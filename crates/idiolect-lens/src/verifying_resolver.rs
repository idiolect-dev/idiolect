//! Content-hash verifying [`Resolver`] wrapper.
//!
//! `PanprotoLens` records carry an `object_hash` field that names the
//! canonical content hash of the lens body. A resolver that fetches a
//! lens over an untrusted transport (a third-party PDS, a caching
//! proxy) could return a body that disagrees with the declared hash —
//! either because of transport corruption or malicious substitution.
//!
//! [`VerifyingResolver`] wraps any [`Resolver`] and re-hashes the
//! returned body against its own `object_hash` on every resolve. On
//! mismatch, the wrapper surfaces [`LensError::Transport`] with a
//! specific message so operators can diagnose.
//!
//! # Hasher pluggability
//!
//! `object_hash` is a string prefixed by the algorithm, e.g.
//! `sha256:deadbeef…`. [`Hasher`] is the trait users implement to
//! cover additional algorithms. This crate ships
//! [`Sha256Hasher`], keyed on the `sha256:` prefix, because that is
//! what every idiolect-authored lens uses today.
//!
//! # What gets hashed
//!
//! The lens's serialized `blob` field, as canonical JSON. This is
//! what panproto computes when it stamps `object_hash` on emit, so
//! the two must agree round-trip if the body was not tampered with.
//! Lenses with an empty blob (rare; intended for placeholder records)
//! are not verifiable and surface [`LensError::Transport`] on resolve.

use idiolect_records::PanprotoLens;

use crate::AtUri;
use crate::error::LensError;
use crate::resolver::Resolver;

/// Compute a content hash of arbitrary bytes.
///
/// Implementations must be deterministic, pure, and produce a
/// hex-encoded digest (no algorithm prefix — the wrapper re-attaches
/// the prefix).
pub trait Hasher: Send + Sync {
    /// The algorithm name, e.g. `"sha256"`. Must match the prefix of
    /// every `object_hash` this hasher is expected to verify.
    fn algorithm(&self) -> &'static str;

    /// Hash `data` and return the hex-encoded digest.
    fn hash_hex(&self, data: &[u8]) -> String;
}

/// SHA-256 hasher suitable for `object_hash` fields that start with
/// `sha256:`. Uses a minimal in-crate SHA-256 implementation that
/// avoids pulling a crypto dep into the core crate; for production
/// use consider the `sha2` crate's impl via a wrapper.
#[derive(Debug, Clone, Default)]
pub struct Sha256Hasher;

// FIPS 180-4 §5.3.3 initial hash value.
#[rustfmt::skip]
const SHA256_IV: [u32; 8] = [
    0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a,
    0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19,
];

// FIPS 180-4 §4.2.2 round constants.
#[rustfmt::skip]
const SHA256_K: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
    0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
    0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
    0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
    0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
    0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
    0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
];

/// One 512-bit block through the compression function. `h` is the
/// running hash state, mutated in place per FIPS 180-4 §6.2.2.
#[allow(clippy::many_single_char_names)]
fn sha256_compress(h: &mut [u32; 8], chunk: &[u8]) {
    let mut w = [0u32; 64];
    for (i, word) in chunk.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA256_K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    h[0] = h[0].wrapping_add(a);
    h[1] = h[1].wrapping_add(b);
    h[2] = h[2].wrapping_add(c);
    h[3] = h[3].wrapping_add(d);
    h[4] = h[4].wrapping_add(e);
    h[5] = h[5].wrapping_add(f);
    h[6] = h[6].wrapping_add(g);
    h[7] = h[7].wrapping_add(hh);
}

impl Hasher for Sha256Hasher {
    fn algorithm(&self) -> &'static str {
        "sha256"
    }

    fn hash_hex(&self, data: &[u8]) -> String {
        // Minimal SHA-256 implementation — no external crypto dep.
        // Based on FIPS 180-4; constant-time is not required because
        // we are hashing public bytes (lens blobs), not secrets.
        let mut h = SHA256_IV;

        // Pre-process: pad message to a multiple of 512 bits.
        let bit_len = (data.len() as u64) * 8;
        let mut msg: Vec<u8> = data.to_vec();
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bit_len.to_be_bytes());

        for chunk in msg.chunks_exact(64) {
            sha256_compress(&mut h, chunk);
        }

        let mut out = String::with_capacity(64);
        for word in h {
            for byte in word.to_be_bytes() {
                use std::fmt::Write;
                write!(&mut out, "{byte:02x}").expect("write to String cannot fail");
            }
        }
        out
    }
}

/// Split `object_hash` into (algorithm, hex digest). Returns `None`
/// for malformed inputs (missing `:` separator).
fn split_hash(object_hash: &str) -> Option<(&str, &str)> {
    object_hash.split_once(':')
}

/// Serialize the lens's blob as canonical bytes suitable for hashing.
///
/// Canonical-JSON semantics: keys sorted alphabetically, no
/// insignificant whitespace, UTF-8. Good enough for idiolect's
/// content addressing; a future move to dag-cbor would require
/// swapping this function.
fn canonical_blob_bytes(lens: &PanprotoLens) -> Result<Vec<u8>, LensError> {
    let blob = lens.blob.as_ref().ok_or_else(|| {
        LensError::Transport(format!(
            "cannot verify hash for lens with no blob (object_hash={})",
            lens.object_hash
        ))
    })?;
    // Canonical form: sort keys via a serde_json::Map reconstruction.
    fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), canonicalize(&map[k]));
                }
                serde_json::Value::Object(out)
            }
            serde_json::Value::Array(a) => {
                serde_json::Value::Array(a.iter().map(canonicalize).collect())
            }
            other => other.clone(),
        }
    }
    let canon = canonicalize(blob);
    serde_json::to_vec(&canon).map_err(|e| LensError::Transport(format!("serialize blob: {e}")))
}

/// Resolver wrapper that rejects returned lenses whose content hash
/// does not match `object_hash`.
pub struct VerifyingResolver<R, H> {
    inner: R,
    hasher: H,
}

impl<R, H> VerifyingResolver<R, H> {
    /// Wrap a resolver with a hasher.
    pub const fn new(inner: R, hasher: H) -> Self {
        Self { inner, hasher }
    }

    /// Borrow the inner resolver.
    pub const fn inner(&self) -> &R {
        &self.inner
    }
}

impl<R> VerifyingResolver<R, Sha256Hasher> {
    /// Construct a verifier using the bundled SHA-256 hasher.
    #[must_use]
    pub const fn sha256(inner: R) -> Self {
        Self::new(inner, Sha256Hasher)
    }
}

impl<R: Resolver, H: Hasher> Resolver for VerifyingResolver<R, H> {
    async fn resolve(&self, uri: &AtUri) -> Result<PanprotoLens, LensError> {
        let lens = self.inner.resolve(uri).await?;

        let (algo, expected) = split_hash(&lens.object_hash).ok_or_else(|| {
            LensError::Transport(format!(
                "malformed object_hash (expected `<algo>:<hex>`): {}",
                lens.object_hash
            ))
        })?;
        if algo != self.hasher.algorithm() {
            return Err(LensError::Transport(format!(
                "object_hash algorithm {algo} does not match verifier algorithm {}",
                self.hasher.algorithm()
            )));
        }

        let bytes = canonical_blob_bytes(&lens)?;
        let actual = self.hasher.hash_hex(&bytes);
        if actual != expected {
            return Err(LensError::Transport(format!(
                "content hash mismatch for {uri}: declared {algo}:{expected}, computed {algo}:{actual}"
            )));
        }

        Ok(lens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::InMemoryResolver;
    use idiolect_records::PanprotoLens;

    fn sha256_hex(data: &[u8]) -> String {
        Sha256Hasher.hash_hex(data)
    }

    #[test]
    fn sha256_known_vectors() {
        // NIST test vectors.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }

    fn valid_lens(blob: serde_json::Value) -> PanprotoLens {
        let bytes = canonical_blob_bytes(&PanprotoLens {
            blob: Some(blob.clone()),
            created_at: "2026-04-21T00:00:00.000Z".into(),
            laws_verified: None,
            object_hash: "sha256:placeholder".into(),
            round_trip_class: None,
            source_schema: "sha256:src".into(),
            target_schema: "sha256:tgt".into(),
        })
        .unwrap();
        let hex = sha256_hex(&bytes);
        PanprotoLens {
            blob: Some(blob),
            created_at: "2026-04-21T00:00:00.000Z".into(),
            laws_verified: None,
            object_hash: format!("sha256:{hex}"),
            round_trip_class: None,
            source_schema: "sha256:src".into(),
            target_schema: "sha256:tgt".into(),
        }
    }

    #[tokio::test]
    async fn verifier_accepts_matching_hash() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l").unwrap();
        let lens = valid_lens(serde_json::json!({ "step": "rename_sort" }));
        let mut inner = InMemoryResolver::new();
        inner.insert(&uri, lens.clone());
        let verifier = VerifyingResolver::sha256(inner);
        let got = verifier.resolve(&uri).await.unwrap();
        assert_eq!(got.object_hash, lens.object_hash);
    }

    #[tokio::test]
    async fn verifier_rejects_mismatched_hash() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/bad").unwrap();
        let mut lens = valid_lens(serde_json::json!({ "step": "rename_sort" }));
        // Corrupt the hash to something random.
        lens.object_hash =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
        let mut inner = InMemoryResolver::new();
        inner.insert(&uri, lens);
        let verifier = VerifyingResolver::sha256(inner);
        let err = verifier.resolve(&uri).await.unwrap_err();
        assert!(matches!(err, LensError::Transport(msg) if msg.contains("content hash mismatch")));
    }

    #[tokio::test]
    async fn verifier_rejects_malformed_hash_string() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l").unwrap();
        let mut lens = valid_lens(serde_json::json!({ "step": "rename_sort" }));
        lens.object_hash = "no-algorithm-prefix".into();
        let mut inner = InMemoryResolver::new();
        inner.insert(&uri, lens);
        let verifier = VerifyingResolver::sha256(inner);
        let err = verifier.resolve(&uri).await.unwrap_err();
        assert!(matches!(err, LensError::Transport(msg) if msg.contains("malformed object_hash")));
    }

    #[tokio::test]
    async fn verifier_rejects_wrong_algorithm() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l").unwrap();
        let mut lens = valid_lens(serde_json::json!({ "step": "rename_sort" }));
        lens.object_hash = "md5:d41d8cd98f00b204e9800998ecf8427e".into();
        let mut inner = InMemoryResolver::new();
        inner.insert(&uri, lens);
        let verifier = VerifyingResolver::sha256(inner);
        let err = verifier.resolve(&uri).await.unwrap_err();
        assert!(
            matches!(err, LensError::Transport(msg) if msg.contains("does not match verifier algorithm"))
        );
    }

    #[tokio::test]
    async fn verifier_rejects_lens_with_no_blob() {
        let uri = crate::AtUri::parse("at://did:plc:x/dev.panproto.schema.lens/l").unwrap();
        let mut inner = InMemoryResolver::new();
        inner.insert(
            &uri,
            PanprotoLens {
                blob: None,
                created_at: "2026-04-21T00:00:00.000Z".into(),
                laws_verified: None,
                object_hash: "sha256:deadbeef".into(),
                round_trip_class: None,
                source_schema: "sha256:src".into(),
                target_schema: "sha256:tgt".into(),
            },
        );
        let verifier = VerifyingResolver::sha256(inner);
        let err = verifier.resolve(&uri).await.unwrap_err();
        assert!(matches!(err, LensError::Transport(msg) if msg.contains("no blob")));
    }

    #[tokio::test]
    async fn verifier_is_canonical_over_key_order() {
        // Two blobs with different key order must hash to the same
        // value so that ReqwestPdsClient's on-the-wire ordering
        // doesn't spuriously fail verification.
        let blob_a = serde_json::json!({ "a": 1, "b": 2 });
        let blob_b = serde_json::json!({ "b": 2, "a": 1 });
        let bytes_a = canonical_blob_bytes(&PanprotoLens {
            blob: Some(blob_a),
            created_at: "x".into(),
            laws_verified: None,
            object_hash: "sha256:_".into(),
            round_trip_class: None,
            source_schema: "sha256:_".into(),
            target_schema: "sha256:_".into(),
        })
        .unwrap();
        let bytes_b = canonical_blob_bytes(&PanprotoLens {
            blob: Some(blob_b),
            created_at: "x".into(),
            laws_verified: None,
            object_hash: "sha256:_".into(),
            round_trip_class: None,
            source_schema: "sha256:_".into(),
            target_schema: "sha256:_".into(),
        })
        .unwrap();
        assert_eq!(bytes_a, bytes_b);
        assert_eq!(sha256_hex(&bytes_a), sha256_hex(&bytes_b));
    }
}
