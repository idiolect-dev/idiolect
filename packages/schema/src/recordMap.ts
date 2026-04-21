// backwards-compat shim over the generated surface.
//
// the canonical nsid → record type map is `RecordTypes` in
// `./generated/records`, emitted by `idiolect-codegen`. this module
// keeps the legacy `RecordMap` alias available so existing imports
// continue to work; prefer `RecordTypes` in new code.

import type { RecordTypes } from "./generated/records";

/** Map each dev.idiolect.* record nsid to its typescript type. */
export type RecordMap = RecordTypes;
