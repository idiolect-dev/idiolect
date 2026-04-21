//! Tests for [`LogPublisher`](idiolect_observer::LogPublisher).
//!
//! The publisher emits one `tracing::info!` event per published
//! observation. We snapshot the tracing output via a per-thread
//! subscriber and assert the expected field names land in the event.

use std::sync::{Arc, Mutex};

use idiolect_observer::{LogPublisher, OBSERVATION_NSID, ObservationPublisher};
use idiolect_records::generated::defs::Visibility;
use idiolect_records::generated::observation::{
    Observation, ObservationMethod as ObservationMethodDescriptor, ObservationScope,
};
use tracing::subscriber::with_default;
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{Layer, Registry};

/// Capture a single formatted event's visit record (key -> value).
#[derive(Default)]
struct CapturedEvent {
    fields: std::collections::BTreeMap<String, String>,
    target: String,
}

#[derive(Default, Clone)]
struct EventSink {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

struct SinkLayer {
    sink: EventSink,
}

struct FieldVisitor<'a> {
    dest: &'a mut std::collections::BTreeMap<String, String>,
}

impl tracing::field::Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.dest
            .insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.dest.insert(field.name().to_owned(), value.to_owned());
    }
}

impl<S> Layer<S> for SinkLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut captured = CapturedEvent {
            target: event.metadata().target().to_owned(),
            ..Default::default()
        };
        let mut visitor = FieldVisitor {
            dest: &mut captured.fields,
        };
        event.record(&mut visitor);
        self.sink.events.lock().unwrap().push(captured);
    }
}

fn fixture_observation() -> Observation {
    Observation {
        method: ObservationMethodDescriptor {
            code_ref: None,
            description: Some("test-desc".to_owned()),
            name: "correction-rate".to_owned(),
            parameters: None,
        },
        observer: "did:plc:observer".to_owned(),
        occurred_at: "2026-04-20T12:00:00Z".to_owned(),
        output: serde_json::json!({ "lenses": {} }),
        scope: ObservationScope {
            communities: None,
            encounter_kinds: None,
            lenses: None,
            window: None,
        },
        version: "1.0.0".to_owned(),
        visibility: Visibility::PublicDetailed,
    }
}

#[test]
fn log_publisher_emits_structured_event_with_body() {
    let sink = EventSink::default();
    let subscriber = Registry::default().with(SinkLayer { sink: sink.clone() });

    let publisher = LogPublisher::new();
    let observation = fixture_observation();

    with_default(subscriber, || {
        // publish runs inside the subscriber scope.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async { publisher.publish(&observation).await.unwrap() });
    });

    let events = sink.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.target, "idiolect_observer::publisher");
    assert_eq!(
        event.fields.get("method").map(String::as_str),
        Some("correction-rate")
    );
    assert_eq!(
        event.fields.get("version").map(String::as_str),
        Some("1.0.0")
    );
    assert_eq!(
        event.fields.get("observer").map(String::as_str),
        Some("did:plc:observer")
    );
    assert_eq!(
        event.fields.get("occurred_at").map(String::as_str),
        Some("2026-04-20T12:00:00Z"),
    );
    // body is included and contains the method name plus the NSID-like
    // suffix for the output payload.
    let body = event.fields.get("body").expect("body present");
    assert!(body.contains("correction-rate"));
    assert!(body.contains("did:plc:observer"));
    // sanity: no OBSERVATION_NSID contamination inside the logged body
    // (the body is the record, not the envelope).
    assert!(!body.contains(OBSERVATION_NSID));
}

#[test]
fn log_publisher_headers_only_omits_body() {
    let sink = EventSink::default();
    let subscriber = Registry::default().with(SinkLayer { sink: sink.clone() });

    let publisher = LogPublisher::headers_only();
    let observation = fixture_observation();

    with_default(subscriber, || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async { publisher.publish(&observation).await.unwrap() });
    });

    let events = sink.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert!(!event.fields.contains_key("body"));
    assert!(event.fields.contains_key("method"));
}
