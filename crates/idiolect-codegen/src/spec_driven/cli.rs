//! CLI subcommand codegen, driven by the orchestrator's query spec.
//!
//! The `idiolect orchestrator …` subcommand family mirrors the
//! catalog queries one-for-one, so it derives from the same
//! `orchestrator-spec/queries.json`. This module reads the parsed
//! spec and emits a dispatcher that maps CLI flags to HTTP paths,
//! eliminating duplicate taxonomy maintenance.
//!
//! Emits one file: `crates/idiolect-cli/src/generated.rs`. The
//! `idiolect-cli` binary composes the generated dispatcher with the
//! hand-written `resolve`, `fetch`, `version`, and `help`
//! subcommands.
//!
//! # Spec fields consumed
//!
//! Each `QueryDecl::cli` field (optional) shapes the dispatcher:
//!
//! - `subcommand: [seg, ...]` — path under `idiolect orchestrator`.
//!   `["bounties"]` means `idiolect orchestrator bounties`.
//! - `flag: name` — optional `--name VALUE` flag. When present, the
//!   flag's value is forwarded as an HTTP query parameter matching
//!   the spec's `http_query`.
//! - `default: true` — the subcommand with no flags falls through
//!   to this query (typical for `bounties` → `/v1/bounties/open`).
//!
//! A query without a `cli` field is not surfaced to the CLI.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use proc_macro2::TokenStream;
use quote::quote;

use super::orchestrator::{ParserKind, QueryDecl, QuerySpec};

/// Emit the CLI's generated dispatcher file into `cli_src`.
///
/// `cli_src` is the `crates/idiolect-cli/src` directory.
pub fn emit(spec: &QuerySpec, cli_src: &Path) -> Result<Vec<PathBuf>> {
    let out_path = cli_src.join("generated.rs");
    std::fs::write(&out_path, emit_source(spec)?)
        .with_context(|| format!("write {}", out_path.display()))?;
    Ok(vec![out_path])
}

fn emit_source(spec: &QuerySpec) -> Result<String> {
    let surfaced: Vec<&QueryDecl> = spec.queries.iter().filter(|q| q.cli.is_some()).collect();

    // Group by subcommand path — same subcommand path, multiple
    // queries differentiated by flag presence/absence.
    let mut groups: BTreeMap<Vec<String>, Vec<&QueryDecl>> = BTreeMap::new();
    for q in &surfaced {
        let cli = q.cli.as_ref().expect("filtered above");
        groups.entry(cli.subcommand.clone()).or_default().push(q);
    }

    let arms: Vec<TokenStream> = groups
        .iter()
        .map(|(subcommand, queries)| dispatch_arm(subcommand, queries))
        .collect();

    let help_lines: Vec<TokenStream> = groups
        .iter()
        .flat_map(|(subcommand, queries)| {
            queries
                .iter()
                .map(move |q| help_line(subcommand, q))
                .collect::<Vec<_>>()
        })
        .collect();

    let items = vec![quote! {
        #![allow(unused_imports, clippy::too_many_lines)]

        use std::process::ExitCode;

        use anyhow::{Result, anyhow, bail};

        /// Dispatch an `idiolect orchestrator <path>` invocation.
        ///
        /// Returns the HTTP path + query string (relative to the configured base URL).
        /// The caller fetches and prints the response.
        #[allow(clippy::missing_errors_doc)]
        pub fn dispatch(
            path: &[&str],
            flags: &std::collections::HashMap<String, String>,
        ) -> Result<String> {
            match path {
                #(#arms,)*
                [other, ..] => bail!("unknown orchestrator subcommand: {other}"),
                [] => bail!("`orchestrator` subcommand required"),
            }
        }

        /// Minimal percent-encoding for CLI flag values forwarded as
        /// HTTP query parameters. Matches the hand-written helper in
        /// `main.rs`.
        fn urlencode(s: &str) -> String {
            let mut out = String::with_capacity(s.len());
            for b in s.bytes() {
                match b {
                    b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        out.push(b as char);
                    }
                    _ => {
                        use std::fmt::Write;
                        write!(&mut out, "%{b:02X}").expect("write to String cannot fail");
                    }
                }
            }
            out
        }

        /// Help text for `idiolect orchestrator …` — shown by the
        /// top-level `help` subcommand.
        #[must_use]
        pub fn help_text() -> String {
            let mut s = String::from("  orchestrator subcommands:\n");
            #(#help_lines)*
            s
        }
    }];

    super::render_file_with_source(
        "CLI dispatcher for `idiolect orchestrator …` subcommands.",
        "orchestrator-spec/queries.json",
        items,
    )
}

/// Build one `[segments] => { … }` arm of the dispatcher's match.
fn dispatch_arm(subcommand: &[String], queries: &[&QueryDecl]) -> TokenStream {
    // Partition into default-binding vs flag-bearing queries.
    let default_query = queries
        .iter()
        .find(|q| q.cli.as_ref().is_some_and(|c| c.default));
    let flagged: Vec<&&QueryDecl> = queries
        .iter()
        .filter(|q| {
            q.cli
                .as_ref()
                .is_some_and(|c| !c.default && c.flag.is_some())
        })
        .collect();

    let flag_branches: Vec<TokenStream> = flagged.iter().map(|q| flag_branch(q)).collect();

    let fallthrough = if let Some(default_q) = default_query {
        if default_q.params.is_empty() {
            let path = &default_q.http.path;
            quote! { Ok(#path.to_owned()) }
        } else {
            let msg = format!(
                "default query {} has params but no flag bindings",
                default_q.name,
            );
            quote! { Err(anyhow!(#msg)) }
        }
    } else if flagged.is_empty() {
        let sub = subcommand.join(" ");
        let msg = format!("`orchestrator {sub}` requires a flag");
        quote! { Err(anyhow!(#msg)) }
    } else {
        let flag_list: Vec<String> = flagged
            .iter()
            .filter_map(|q| q.cli.as_ref().and_then(|c| c.flag.clone()))
            .map(|f| format!("--{f}"))
            .collect();
        let list = flag_list.join(", ");
        let sub = subcommand.join(" ");
        let msg = format!("`orchestrator {sub}` requires one of: {list}");
        quote! { Err(anyhow!(#msg)) }
    };

    // Match pattern: `["bounties"]`, `["adapters"]`, etc.
    let segments: Vec<TokenStream> = subcommand.iter().map(|s| quote! { #s }).collect();

    quote! {
        [#(#segments),*] => {
            #(#flag_branches)*
            #fallthrough
        }
    }
}

/// Body of a single `--flag` branch inside a dispatch arm.
fn flag_branch(q: &QueryDecl) -> TokenStream {
    let cli = q.cli.as_ref().expect("cli present");
    let flag = cli.flag.as_ref().expect("flagged query has flag");
    let param = q
        .params
        .iter()
        .find(|p| &p.name == flag || &p.http_query == flag)
        .unwrap_or_else(|| panic!("flag {flag} has no matching param on {}", q.name));
    let path = &q.http.path;
    let http_query = &param.http_query;
    let template = format!("{path}?{http_query}={{}}");
    let flag_lit = flag.as_str();
    quote! {
        if let Some(value) = flags.get(#flag_lit) {
            return Ok(format!(#template, urlencode(value)));
        }
    }
}

/// One `s.push_str("    orchestrator sub --flag VALUE   desc\n");` line.
fn help_line(subcommand: &[String], q: &QueryDecl) -> TokenStream {
    let cli = q.cli.as_ref().expect("cli present");
    let sub = subcommand.join(" ");
    let flag_part = match &cli.flag {
        Some(f) => {
            let param = q
                .params
                .iter()
                .find(|p| &p.name == f || &p.http_query == f)
                .expect("flag has matching param");
            let value_label = match param.parser {
                ParserKind::SchemaRefFromUri | ParserKind::LensRefFromUri => "AT_URI",
                ParserKind::VerificationKind | ParserKind::AdapterInvocationProtocolKind => "KIND",
                ParserKind::String => "VALUE",
            };
            format!(" --{f} {value_label}")
        }
        None if cli.default => String::new(),
        None => String::from(" (no flag)"),
    };
    let desc = q
        .description
        .as_deref()
        .map_or("", |d| d.lines().next().unwrap_or(""));
    let line = format!("    orchestrator {sub}{flag_part}   {desc}\n");
    quote! {
        s.push_str(#line);
    }
}
