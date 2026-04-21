//! Tests for the expiry helpers on [`OAuthSession`].

use idiolect_oauth::OAuthSession;
use time::{Duration, OffsetDateTime, macros::datetime};

fn fresh_session(expires_at: &str) -> OAuthSession {
    OAuthSession {
        did: "did:plc:alice".into(),
        pds_url: "https://pds.example".into(),
        access_jwt: "jwt".into(),
        refresh_jwt: "rjwt".into(),
        dpop_private_key_jwk: "jwk".into(),
        dpop_nonce: None,
        token_type: Some("DPoP".into()),
        scope: None,
        handle: Some("alice.example".into()),
        issued_at: "2026-04-21T00:00:00Z".into(),
        expires_at: expires_at.into(),
        refresh_expires_at: None,
    }
}

#[test]
fn is_expired_reports_past_and_future_correctly() {
    let now = datetime!(2026-04-21 12:00 UTC);
    let future = fresh_session("2026-04-21T13:00:00Z");
    let past = fresh_session("2026-04-21T11:00:00Z");
    let exact = fresh_session("2026-04-21T12:00:00Z");
    assert!(!future.is_expired(now));
    assert!(past.is_expired(now));
    // `now >= t` so exact equality counts as expired — matches the
    // behavior every JWT validator in the wild uses (an access token
    // at its exp is unusable).
    assert!(exact.is_expired(now));
}

#[test]
fn is_expired_treats_malformed_timestamp_as_expired() {
    let now = datetime!(2026-04-21 12:00 UTC);
    let s = fresh_session("not-a-timestamp");
    assert!(s.is_expired(now));
}

#[test]
fn time_until_expiry_returns_positive_duration_for_future() {
    let now = datetime!(2026-04-21 12:00 UTC);
    let s = fresh_session("2026-04-21T13:30:00Z");
    assert_eq!(s.time_until_expiry(now), Duration::minutes(90));
}

#[test]
fn time_until_expiry_is_zero_for_past() {
    let now = datetime!(2026-04-21 12:00 UTC);
    let s = fresh_session("2026-04-21T11:00:00Z");
    assert_eq!(s.time_until_expiry(now), Duration::ZERO);
}

#[test]
fn needs_refresh_uses_threshold() {
    let now = datetime!(2026-04-21 12:00 UTC);
    // Token expires 10 min from now. Threshold 5 min => no refresh yet.
    let s = fresh_session("2026-04-21T12:10:00Z");
    assert!(!s.needs_refresh(now, Duration::minutes(5)));
    // Threshold 15 min => refresh now.
    assert!(s.needs_refresh(now, Duration::minutes(15)));
}

#[test]
fn needs_refresh_true_for_malformed_timestamp() {
    let now = datetime!(2026-04-21 12:00 UTC);
    let s = fresh_session("bogus");
    assert!(s.needs_refresh(now, Duration::minutes(5)));
}

#[test]
fn refresh_expired_handles_unset_and_set_cases() {
    let now = datetime!(2026-04-21 12:00 UTC);
    let mut s = fresh_session("2026-04-21T13:00:00Z");
    // Unset refresh_expires_at: not expired (no deadline declared).
    assert!(!s.refresh_expired(now));
    // Set to the past: expired.
    s.refresh_expires_at = Some("2026-04-20T00:00:00Z".into());
    assert!(s.refresh_expired(now));
    // Set to the future: not expired.
    s.refresh_expires_at = Some("2026-05-01T00:00:00Z".into());
    assert!(!s.refresh_expired(now));
    // Malformed: fall back to not-expired (consistent with "no deadline").
    s.refresh_expires_at = Some("not-a-timestamp".into());
    assert!(!s.refresh_expired(now));
}

#[test]
fn offset_other_than_utc_is_supported() {
    let now = datetime!(2026-04-21 12:00 UTC);
    // 2026-04-21T08:00:00-04:00 == 2026-04-21T12:00:00Z, so exactly
    // expired.
    let s = fresh_session("2026-04-21T08:00:00-04:00");
    assert!(s.is_expired(now));
}

#[test]
fn round_trip_through_time_format() {
    // Sanity check: OffsetDateTime::now_utc() formatted via Rfc3339
    // is itself parseable.
    let now = OffsetDateTime::now_utc();
    let formatted = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let s = fresh_session(&formatted);
    assert!(s.is_expired(now));
    assert!(!s.is_expired(now - Duration::seconds(1)));
}
