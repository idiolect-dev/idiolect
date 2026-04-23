//! ES256-backed [`DpopProver`](crate::DpopProver).
//!
//! Implements `DPoP` proof construction per RFC 9449: a signed JWT
//! whose header declares `typ=dpop+jwt` + `alg=ES256` + the public
//! key JWK, and whose payload claims `htm` (HTTP method), `htu`
//! (HTTP URL), `iat`, `jti`, optional `nonce`, and `ath` (base64url
//! of sha256(access token)).
//!
//! # Feature gate
//!
//! Enabled via the `dpop-p256` crate feature so callers that do not
//! need ES256 signing do not pull in the crypto dependency tree.
//!
//! # Key management
//!
//! [`P256DpopProver::generate`] mints a fresh P-256 signing key; use
//! it for short-lived sessions. [`P256DpopProver::from_pkcs8_pem`]
//! loads a caller-managed key. The private key stays in memory —
//! persist it through your own encrypted storage layer.
//!
//! # JWK
//!
//! The public half of the key is serialized as a JWK (`kty=EC`,
//! `crv=P-256`, `x`, `y`) and embedded in every proof's header so
//! the server does not need to pre-register the key. Callers that
//! *do* pre-register (OAuth `DPoP` with binding) reuse the same
//! `jwk()` output for registration.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::SecretKey;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::DecodePrivateKey;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::signing_writer::DpopProver;

/// P-256 ES256 `DPoP` prover.
pub struct P256DpopProver {
    signing: SigningKey,
    public_jwk: serde_json::Value,
}

impl std::fmt::Debug for P256DpopProver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("P256DpopProver")
            .field("public_jwk", &self.public_jwk)
            .finish_non_exhaustive()
    }
}

impl P256DpopProver {
    /// Mint a fresh P-256 key pair and wrap it in a prover.
    ///
    /// Uses the thread-local OS rng. The private key is never
    /// exposed off the [`P256DpopProver`] value — persist the pkcs8
    /// PEM via [`to_pkcs8_pem`](Self::to_pkcs8_pem) if you need to
    /// retain it.
    #[must_use]
    pub fn generate() -> Self {
        let signing = SigningKey::random(&mut rand::thread_rng());
        let public_jwk = public_jwk_from(&signing);
        Self {
            signing,
            public_jwk,
        }
    }

    /// Load a pre-existing P-256 key from its PKCS#8 PEM encoding.
    ///
    /// # Errors
    ///
    /// Returns a descriptive string on parse failure.
    pub fn from_pkcs8_pem(pem: &str) -> Result<Self, String> {
        let secret = SecretKey::from_pkcs8_pem(pem).map_err(|e| format!("pkcs8 parse: {e}"))?;
        let signing = SigningKey::from(&secret);
        let public_jwk = public_jwk_from(&signing);
        Ok(Self {
            signing,
            public_jwk,
        })
    }

    /// Public-half JWK (`{kty, crv, x, y}`) suitable for `DPoP`-binding
    /// registration at an authorization server.
    #[must_use]
    pub fn public_jwk(&self) -> &serde_json::Value {
        &self.public_jwk
    }

    /// Construct a pkcs8-PEM serialization of the private key.
    ///
    /// # Errors
    ///
    /// Returns a descriptive string on encoding failure.
    pub fn to_pkcs8_pem(&self) -> Result<String, String> {
        use p256::pkcs8::EncodePrivateKey;
        let secret = SecretKey::from(&self.signing);
        let pem = secret
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .map_err(|e| format!("pkcs8 encode: {e}"))?;
        Ok(pem.to_string())
    }
}

impl DpopProver for P256DpopProver {
    fn proof(
        &self,
        method: &str,
        url: &str,
        access_token: &str,
        nonce: Option<&str>,
    ) -> Result<String, String> {
        let header = serde_json::json!({
            "alg": "ES256",
            "typ": "dpop+jwt",
            "jwk": self.public_jwk,
        });
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("clock: {e}"))?
            .as_secs();

        // jti: 128 random bits, base64url-encoded.
        let mut jti_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut jti_bytes);
        let jti = URL_SAFE_NO_PAD.encode(jti_bytes);

        // ath: base64url(sha256(access_token)).
        let ath = {
            let mut h = Sha256::new();
            h.update(access_token.as_bytes());
            URL_SAFE_NO_PAD.encode(h.finalize())
        };

        let mut payload = serde_json::json!({
            "htm": method,
            "htu": url,
            "iat": now,
            "jti": jti,
            "ath": ath,
        });
        if let Some(n) = nonce {
            payload["nonce"] = serde_json::Value::String(n.to_owned());
        }

        let header_b64 = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).map_err(|e| format!("serialize header: {e}"))?);
        let payload_b64 = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).map_err(|e| format!("serialize payload: {e}"))?);
        let signing_input = format!("{header_b64}.{payload_b64}");

        let signature: Signature = self.signing.sign(signing_input.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        Ok(format!("{signing_input}.{sig_b64}"))
    }
}

/// Build a `{kty, crv, x, y}` JWK from a `SigningKey`'s public
/// component.
fn public_jwk_from(signing: &SigningKey) -> serde_json::Value {
    let public = signing.verifying_key();
    let point = public.to_encoded_point(false);
    let x = point.x().expect("P-256 public key has x coordinate");
    let y = point.y().expect("P-256 public key has y coordinate");
    serde_json::json!({
        "kty": "EC",
        "crv": "P-256",
        "x": URL_SAFE_NO_PAD.encode(x),
        "y": URL_SAFE_NO_PAD.encode(y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::VerifyingKey;
    use p256::ecdsa::signature::Verifier;

    #[test]
    fn generated_prover_produces_verifiable_proof() {
        let prover = P256DpopProver::generate();
        let proof = prover
            .proof(
                "POST",
                "https://pds.example/xrpc/com.atproto.repo.createRecord",
                "tok",
                None,
            )
            .unwrap();
        // Split the JWT.
        let parts: Vec<&str> = proof.split('.').collect();
        assert_eq!(parts.len(), 3);

        // Header decodes and claims ES256.
        let header_bytes = URL_SAFE_NO_PAD.decode(parts[0]).unwrap();
        let header: serde_json::Value = serde_json::from_slice(&header_bytes).unwrap();
        assert_eq!(header["alg"], "ES256");
        assert_eq!(header["typ"], "dpop+jwt");
        assert_eq!(header["jwk"]["crv"], "P-256");

        // Payload carries the expected claims.
        let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1]).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert_eq!(payload["htm"], "POST");
        assert_eq!(
            payload["htu"],
            "https://pds.example/xrpc/com.atproto.repo.createRecord"
        );
        assert!(payload["iat"].as_u64().is_some());
        assert!(payload["jti"].is_string());
        assert!(payload["ath"].is_string());
        assert!(payload["nonce"].is_null());

        // Signature verifies against the embedded public key.
        let verifying: &VerifyingKey = prover.signing.verifying_key();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        let sig = Signature::from_slice(&sig_bytes).unwrap();
        verifying.verify(signing_input.as_bytes(), &sig).unwrap();
    }

    #[test]
    fn nonce_appears_in_payload_when_supplied() {
        let prover = P256DpopProver::generate();
        let proof = prover
            .proof(
                "POST",
                "https://pds.example/x",
                "tok",
                Some("server-nonce-1"),
            )
            .unwrap();
        let parts: Vec<&str> = proof.split('.').collect();
        let payload: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(payload["nonce"], "server-nonce-1");
    }

    #[test]
    fn pkcs8_pem_round_trip_preserves_jwk() {
        let original = P256DpopProver::generate();
        let pem = original.to_pkcs8_pem().unwrap();
        let loaded = P256DpopProver::from_pkcs8_pem(&pem).unwrap();
        // Same key => same JWK.
        assert_eq!(original.public_jwk(), loaded.public_jwk());
    }

    #[test]
    fn ath_claim_is_base64_sha256_of_token() {
        let prover = P256DpopProver::generate();
        let proof = prover.proof("GET", "https://x", "my-token", None).unwrap();
        let parts: Vec<&str> = proof.split('.').collect();
        let payload: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        let expected = {
            let mut h = Sha256::new();
            h.update(b"my-token");
            URL_SAFE_NO_PAD.encode(h.finalize())
        };
        assert_eq!(payload["ath"], expected);
    }
}
