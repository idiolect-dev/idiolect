//! The [`ObservationMethod`] trait — the pluggable aggregator.
//!
//! An observer is defined by a *method*: a stateful function that
//! folds encounter-family records (encounters, corrections,
//! retrospections, verifications) into a running aggregate, and can be
//! asked to emit a `dev.idiolect.observation` snapshot at any point.
//!
//! Methods are the unit of governance on the read side of idiolect:
//! the observer picks a method, declares its scope, and publishes
//! snapshots keyed by that method's identity. Two observers running
//! different methods over the same firehose publish non-comparable
//! observations — that is by design (P6 — "decouple ranking from the
//! orchestrator").

use idiolect_indexer::IndexerEvent;
use idiolect_records::generated::defs::Visibility;
use idiolect_records::generated::observation::{
    Observation, ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};

use crate::error::ObserverResult;

/// A pluggable aggregation method.
///
/// Implementations carry their own state. The [`observe`](Self::observe)
/// method is called once per decoded firehose event that the observer
/// has decided is in-scope; [`snapshot`](Self::snapshot) produces the
/// *output* payload of the next `dev.idiolect.observation` record.
///
/// The default [`descriptor`](Self::descriptor) and
/// [`scope`](Self::scope) implementations are pure accessors — they do
/// not touch aggregate state — so they can be called from the
/// publisher side without coordination.
///
/// `Send + Sync` is required because the observer runtime may run a
/// flush task on a separate tokio task while the indexer continues to
/// push events into the aggregator. Implementations with non-trivial
/// mutation therefore typically hold their state behind a mutex or
/// use interior-mutable atomics; [`observe`](Self::observe) takes
/// `&mut self` so the outer runtime can serialize access when needed.
pub trait ObservationMethod: Send + Sync {
    /// A short, stable identifier for this method.
    ///
    /// Shows up in every published observation's `method.name`; two
    /// observers publishing under the same name are asserting their
    /// outputs are comparable.
    fn name(&self) -> &str;

    /// A version string. Observers bump this when the method's
    /// semantics change in a way that invalidates comparison with
    /// older snapshots.
    fn version(&self) -> &str;

    /// The method descriptor embedded in every emitted observation.
    ///
    /// Default: `{ name, description, codeRef, parameters }` where
    /// `description`, `codeRef`, and `parameters` are `None`. Override
    /// to expose method-specific knobs.
    fn descriptor(&self) -> ObservationMethodDescriptor {
        ObservationMethodDescriptor {
            code_ref: None,
            description: None,
            name: self.name().to_owned(),
            parameters: None,
        }
    }

    /// The scope this observer declares.
    ///
    /// Default: an empty scope (all encounter kinds, all lenses, all
    /// communities, all time). Override to narrow.
    fn scope(&self) -> ObservationScope {
        ObservationScope {
            communities: None,
            encounter_kinds: None,
            lenses: None,
            window: None,
        }
    }

    /// Fold a single in-scope event into the aggregator's state.
    ///
    /// The observer runtime has already decoded the record into an
    /// [`idiolect_records::AnyRecord`] variant and handed it in via
    /// the [`IndexerEvent`]. Methods that only care about certain
    /// record kinds should pattern-match on `event.record` and ignore
    /// the rest.
    ///
    /// # Errors
    ///
    /// Return [`crate::ObserverError::Method`] via the `ObserverResult`
    /// alias when an invariant the method cares about is violated
    /// (e.g. duplicate sequence id, malformed payload the lexicon
    /// admits but the method does not). The runtime treats method
    /// errors as fatal by default; deployments that want tolerant
    /// behavior should swallow the error inside the method itself.
    fn observe(&mut self, event: &IndexerEvent) -> ObserverResult<()>;

    /// Produce the current snapshot of the method's output.
    ///
    /// The runtime will wrap this output in an [`Observation`] record
    /// keyed by the method's [`descriptor`](Self::descriptor) and
    /// [`scope`](Self::scope) and hand the result to the publisher.
    ///
    /// Methods may return `None` to indicate they have nothing useful
    /// to publish yet (e.g. insufficient data). The runtime skips
    /// publication for that cycle.
    ///
    /// # Errors
    ///
    /// Return [`crate::ObserverError::Method`] if producing the
    /// snapshot fails in a recoverable way (bad internal invariant,
    /// downstream serialization mismatch).
    fn snapshot(&self) -> ObserverResult<Option<serde_json::Value>>;
}

/// Glue the method's descriptor, scope, and current snapshot into a
/// ready-to-publish [`Observation`] record body.
///
/// The `observer` did and `occurred_at` timestamp are caller-supplied
/// because they depend on deployment identity and the publisher's
/// clock, neither of which is a method concern.
///
/// # Errors
///
/// Returns [`crate::ObserverError::Method`] if the method's
/// `snapshot` call fails.
pub fn build_observation<M: ObservationMethod + ?Sized>(
    method: &M,
    observer_did: &str,
    occurred_at: &str,
    visibility: Visibility,
) -> ObserverResult<Option<Observation>> {
    let Some(output) = method.snapshot()? else {
        return Ok(None);
    };

    Ok(Some(Observation {
        basis: None,
        method: method.descriptor(),
        observer: observer_did.to_owned(),
        occurred_at: occurred_at.to_owned(),
        output,
        scope: method.scope(),
        version: method.version().to_owned(),
        visibility,
    }))
}
