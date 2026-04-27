//! Rust target: family.rs emitter.
//!
//! Walks the record-type lexicons in scope and emits the per-family
//! [`AnyRecord`] enum, [`decode_record`] dispatch, and
//! [`RecordFamily`] impl. The hand-written counterparts in
//! `idiolect-records/src/record.rs` become thin re-exports of the
//! generated module.
//!
//! Family scope is determined by `family_membership_nsid_prefix`:
//! records whose NSID starts with that prefix are members. The
//! default for `idiolect-codegen` is `"dev.idiolect"`. Downstream
//! consumers running their own codegen instance (e.g. `layers-pub`
//! over `pub.layers.*`) point this at their own prefix.

use anyhow::Result;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::lexicon::{Def, LexiconDoc, module_name_for_nsid};

use super::rust::pascal_case;

/// Configuration for a single family.
pub struct FamilyConfig {
    /// Marker type name (e.g. `IdiolectFamily`).
    pub marker_name: &'static str,
    /// Family ID exposed via `RecordFamily::ID`.
    pub id: &'static str,
    /// NSID prefix used for membership. Records whose NSID does
    /// not start with this string are not part of the family.
    pub nsid_prefix: &'static str,
}

/// Default family the `dev.idiolect.*` codegen emits.
pub const IDIOLECT_FAMILY: FamilyConfig = FamilyConfig {
    marker_name: "IdiolectFamily",
    id: "dev.idiolect",
    nsid_prefix: "dev.idiolect.",
};

/// One member of a family — the precomputed bag of token-trees the
/// emitter splices into the generated module.
struct Entry {
    nsid: String,
    ident: syn::Ident,
    ty: syn::Type,
    nsid_path: syn::Path,
}

/// Render the body of `generated/family.rs`.
///
/// # Errors
///
/// Returns an [`anyhow::Error`] if the generated token stream fails
/// to parse as a `syn::File`. That only happens for an internal
/// emitter bug (a malformed quote block); the user-supplied input
/// (lexicon docs) cannot trigger it.
///
/// # Panics
///
/// Panics if any record-type lexicon's NSID fails to convert into a
/// valid Rust type path or NSID const path. Codegen-emitted record
/// modules always satisfy this invariant; the panic is reserved for
/// a structural bug elsewhere in the codegen pipeline.
#[allow(clippy::too_many_lines)]
pub fn render_family_rs(docs: &[LexiconDoc], cfg: &FamilyConfig) -> Result<String> {
    let entries: Vec<Entry> = collect_entries(docs, cfg);

    let marker = format_ident!("{}", cfg.marker_name);
    let id_lit = cfg.id;

    let variants = entries.iter().map(|e| {
        let doc = format!(" A `{}` record.", e.nsid);
        let ident = &e.ident;
        let ty = &e.ty;
        quote! {
            #[doc = #doc]
            #ident(#ty)
        }
    });

    let nsid_str_arms = entries.iter().map(|e| {
        let ident = &e.ident;
        let nsid_path = &e.nsid_path;
        quote! { Self::#ident(_) => #nsid_path }
    });

    let inner_to_json_arms = entries.iter().map(|e| {
        let ident = &e.ident;
        quote! { Self::#ident(r) => serde_json::to_value(r) }
    });

    let decode_arms = entries.iter().map(|e| {
        let ident = &e.ident;
        let nsid_path = &e.nsid_path;
        quote! { s if s == #nsid_path => Ok(AnyRecord::#ident(from(value)?)) }
    });

    let contains_first = entries.first().map(|e| e.nsid_path.clone());
    let contains_rest = entries.iter().skip(1).map(|e| e.nsid_path.clone());

    let marker_doc = format!(
        " Marker type for the `{id_lit}` family. Implementing\n [`RecordFamily`] makes the family first-class alongside any\n downstream-curated family or composed [`OrFamily`](crate::OrFamily).",
    );

    let output: TokenStream = quote! {
        #![allow(clippy::large_enum_variant)]

        use crate::Nsid;
        use crate::Record;
        use crate::family::RecordFamily;
        use crate::record::DecodeError;
        use serde::{Serialize, Serializer};

        #[doc = #marker_doc]
        #[derive(Debug, Clone, Copy)]
        pub struct #marker;

        /// Discriminated-union view over every record type in the
        /// family. Produced by [`decode_record`] when an appview
        /// receives JSON whose NSID is only known at runtime
        /// (e.g. firehose traffic).
        #[derive(Debug, Clone)]
        pub enum AnyRecord {
            #(#variants),*
        }

        impl AnyRecord {
            /// Canonical NSID string of the contained record.
            #[must_use]
            pub const fn nsid_str(&self) -> &'static str {
                match self {
                    #(#nsid_str_arms),*
                }
            }

            /// Typed NSID of the contained record. Parses
            /// [`Self::nsid_str`] each call.
            ///
            /// # Panics
            ///
            /// Panics if the underlying record's `Record::NSID`
            /// constant is not a valid atproto NSID. Codegen emits
            /// per-record unit tests proving this never panics in
            /// practice.
            #[must_use]
            pub fn nsid(&self) -> Nsid {
                Nsid::parse(self.nsid_str())
                    .expect("Record::NSID must be a valid atproto NSID")
            }

            /// Serialize the inner record into a `serde_json::Value`
            /// and splice in a `$type` field carrying [`Self::nsid`].
            /// This is the wire form atproto records take when
            /// serialized into a `com.atproto.repo.*` xrpc request or
            /// a firehose frame.
            ///
            /// # Errors
            ///
            /// [`serde_json::Error`] when the inner record fails to
            /// serialize, or when its serialized form is not a JSON
            /// object (which never happens for generated record types).
            pub fn to_typed_json(&self) -> Result<serde_json::Value, serde_json::Error> {
                let mut value = self.inner_to_json()?;
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "$type".to_owned(),
                        serde_json::Value::String(self.nsid_str().to_owned()),
                    );
                    Ok(value)
                } else {
                    Err(serde::ser::Error::custom(
                        "record did not serialize to a json object",
                    ))
                }
            }

            /// Decode a JSON value into the variant identified by its
            /// embedded `$type` field. Internally reads `$type` and
            /// delegates to [`decode_record`].
            ///
            /// # Errors
            ///
            /// [`DecodeError::UnknownNsid`] when `$type` is missing,
            /// not a string, or not a known NSID.
            /// [`DecodeError::Serde`] when the body fails to match
            /// the variant the `$type` selects.
            pub fn from_typed_json(mut value: serde_json::Value) -> Result<Self, DecodeError> {
                let Some(serde_json::Value::String(nsid_str)) =
                    value.as_object_mut().and_then(|o| o.remove("$type"))
                else {
                    return Err(DecodeError::UnknownNsid(
                        "<missing $type field>".to_owned(),
                    ));
                };
                let nsid = Nsid::parse(&nsid_str)
                    .map_err(|_| DecodeError::UnknownNsid(nsid_str))?;
                decode_record(&nsid, value)
            }

            fn inner_to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
                match self {
                    #(#inner_to_json_arms),*
                }
            }
        }

        /// Serialize an [`AnyRecord`] in its typed-json wire form.
        /// PDSes expect the union discriminator inside the object via
        /// `$type`.
        impl Serialize for AnyRecord {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let value = self.to_typed_json()
                    .map_err(serde::ser::Error::custom)?;
                value.serialize(serializer)
            }
        }

        impl std::fmt::Display for AnyRecord {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "AnyRecord({})", self.nsid_str())
            }
        }

        /// Decode a JSON value into the [`AnyRecord`] variant
        /// selected by `nsid`.
        ///
        /// # Errors
        ///
        /// [`DecodeError::UnknownNsid`] if `nsid` is not a member of
        /// this family. [`DecodeError::Serde`] if `value` does not
        /// deserialize into the record type selected by `nsid`.
        pub fn decode_record(
            nsid: &Nsid,
            value: serde_json::Value,
        ) -> Result<AnyRecord, DecodeError> {
            fn from<R: Record>(value: serde_json::Value) -> Result<R, DecodeError> {
                serde_json::from_value(value).map_err(DecodeError::Serde)
            }
            let s = nsid.as_str();
            match s {
                #(#decode_arms),*,
                other => Err(DecodeError::UnknownNsid(other.to_owned())),
            }
        }

        impl RecordFamily for #marker {
            type AnyRecord = AnyRecord;

            const ID: &'static str = #id_lit;

            fn contains(nsid: &Nsid) -> bool {
                matches!(
                    nsid.as_str(),
                    #contains_first #(| #contains_rest)*
                )
            }

            fn decode(
                nsid: &Nsid,
                body: serde_json::Value,
            ) -> Result<Option<Self::AnyRecord>, DecodeError> {
                if !Self::contains(nsid) {
                    return Ok(None);
                }
                decode_record(nsid, body).map(Some)
            }

            fn nsid_str(record: &Self::AnyRecord) -> &'static str {
                record.nsid_str()
            }

            fn to_typed_json(
                record: &Self::AnyRecord,
            ) -> Result<serde_json::Value, serde_json::Error> {
                record.to_typed_json()
            }
        }
    };

    let file = syn::parse2::<syn::File>(output)
        .map_err(|e| anyhow::anyhow!("family.rs ast parse: {e}"))?;
    let body = prettyplease::unparse(&file);
    let banner = format!(
        "// @generated by idiolect-codegen. do not edit.\n\n//! Generated record family for `{id_lit}`.\n//!\n//! Per-record types come from sibling generated modules; this\n//! file emits the discriminated-union view, the dispatch\n//! function, and the [`RecordFamily`] impl that lets every\n//! family-agnostic boundary in the workspace work with this\n//! family the same way it works with any other.\n\n",
    );
    Ok(format!("{banner}{body}"))
}

fn collect_entries(docs: &[LexiconDoc], cfg: &FamilyConfig) -> Vec<Entry> {
    docs.iter()
        .filter(|d| matches!(d.defs.get("main"), Some(Def::Record(_))))
        .filter(|d| d.nsid.starts_with(cfg.nsid_prefix))
        .map(|doc| {
            let module = module_name_for_nsid(&doc.nsid);
            let ty_name = pascal_case(&module);
            let ident = format_ident!("{}", ty_name);
            let ty: syn::Type =
                syn::parse_str(&format!("crate::{ty_name}")).expect("generated type path parses");
            let nsid_path: syn::Path = syn::parse_str(&format!("crate::{ty_name}::NSID"))
                .expect("generated NSID path parses");
            Entry {
                nsid: doc.nsid.clone(),
                ident,
                ty,
                nsid_path,
            }
        })
        .collect()
}
