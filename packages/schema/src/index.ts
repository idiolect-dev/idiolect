// public entry point for @idiolect-dev/schema.
//
// types are generated from the canonical lexicon json in
// `lexicons/dev/idiolect/*.json` by `idiolect-codegen`. do not edit
// `./generated/` by hand; re-run `cargo run -p idiolect-codegen` after
// changing any lexicon.

// Typed parsed fixture records, mirroring the rust
// `idiolect_records::generated::examples` module.
export {
  adapterExample,
  bountyExample,
  communityExample,
  correctionExample,
  dialectExample,
  encounterExample,
  examplesByNsid,
  observationExample,
  recommendationExample,
  retrospectionExample,
  verificationExample,
} from "./examples";
export * from "./generated/index";
export { buildLexicons, defaultLexicons, loadLexiconDocs } from "./lexicons";
export * from "./nsids";
export * from "./recordMap";
export * from "./validators";
