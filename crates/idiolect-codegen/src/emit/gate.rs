//! Post-emit parse gate over generated sources.
//!
//! Every file a target emits must re-parse cleanly through the
//! matching tree-sitter grammar in `panproto-parse`. The gate parses
//! each emitted file with [`ParserRegistry::parse_with_protocol`] and
//! rejects any schema that carries an `ERROR` or `MISSING` vertex
//! (tree-sitter's recovery kinds), so a renderer defect that produces
//! token soup fails codegen instead of landing in the tree.
//!
//! This is an independent check, not a substitute for the emitters'
//! own invariants: the rust target's `syn::File` and the typescript
//! target's oxc ast guarantee well-formed output by construction, but
//! both targets also stitch file-level fixed bits as text, and the
//! stitched seams are exactly where a regression would hide.

use std::sync::OnceLock;

use panproto_parse::ParserRegistry;

/// Stack reserve for panproto-parse work. The grammar walker and the
/// de-novo emitter both recurse through tree-sitter production trees;
/// panproto's own test suite runs rust/typescript work on 32 MiB
/// workers, so the gate and the pilot target do the same.
pub(crate) const PANPROTO_STACK_BYTES: usize = 32 * 1024 * 1024;

/// Shared parser registry. Construction compiles per-grammar lookup
/// tables, so build it once per process; only the `lang-rust` and
/// `lang-typescript` grammars are enabled in `Cargo.toml`.
pub fn shared_registry() -> &'static ParserRegistry {
    static REGISTRY: OnceLock<ParserRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ParserRegistry::new)
}

/// Errors raised by the emit gate.
#[derive(Debug, thiserror::Error)]
pub enum GateError {
    /// The grammar rejected the file outright.
    #[error("emit gate: {path} failed to parse as {protocol}: {source}")]
    Parse {
        /// Protocol the file was parsed under.
        protocol: &'static str,
        /// Emitted file path (relative to the target's output dir).
        path: String,
        /// Underlying parser error, boxed to keep the gate's result
        /// pointer-sized on the success path.
        #[source]
        source: Box<panproto_parse::ParseError>,
    },

    /// The file parsed, but tree-sitter had to recover: the schema
    /// carries `ERROR` / `MISSING` vertices.
    #[error(
        "emit gate: {path} re-parses with {error_count} ERROR/MISSING \
         vertex(es) under the {protocol} grammar"
    )]
    ErrorVertices {
        /// Protocol the file was parsed under.
        protocol: &'static str,
        /// Emitted file path (relative to the target's output dir).
        path: String,
        /// Number of recovery vertices found.
        error_count: usize,
    },

    /// The gate's worker thread could not run to completion. Nothing
    /// was verified; treated as a failure so the gate never passes by
    /// being skipped.
    #[error("emit gate: {protocol} worker {reason}")]
    Worker {
        /// Protocol the batch was meant to be parsed under.
        protocol: &'static str,
        /// What went wrong with the worker.
        reason: &'static str,
    },
}

/// Parse every `(path, contents)` pair under `protocol` and fail on
/// the first file that does not re-parse cleanly.
///
/// The batch runs on a worker thread with the panproto stack reserve;
/// the scoped spawn keeps the borrowing signature.
///
/// # Errors
///
/// Returns [`GateError::Parse`] when the grammar rejects a file and
/// [`GateError::ErrorVertices`] when a file parses only via
/// tree-sitter's error recovery.
pub fn verify_parses<'a>(
    protocol: &'static str,
    files: impl IntoIterator<Item = (&'a str, &'a str)> + Send,
) -> Result<(), GateError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(PANPROTO_STACK_BYTES)
            .spawn_scoped(scope, move || verify_parses_inner(protocol, files))
            .map_err(|_| GateError::Worker {
                protocol,
                reason: "failed to spawn",
            })?
            .join()
            .map_err(|_| GateError::Worker {
                protocol,
                reason: "panicked",
            })?
    })
}

fn verify_parses_inner<'a>(
    protocol: &'static str,
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), GateError> {
    let registry = shared_registry();
    for (path, contents) in files {
        let schema = registry
            .parse_with_protocol(protocol, contents.as_bytes(), path)
            .map_err(|source| GateError::Parse {
                protocol,
                path: path.to_owned(),
                source: Box::new(source),
            })?;
        let error_count = schema
            .vertices
            .values()
            .filter(|v| {
                let kind: &str = v.kind.as_ref();
                kind == "MISSING" || kind.contains("ERROR")
            })
            .count();
        if error_count > 0 {
            return Err(GateError::ErrorVertices {
                protocol,
                path: path.to_owned(),
                error_count,
            });
        }
    }
    Ok(())
}
