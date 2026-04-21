// backwards-compat shim over the generated surface.
//
// the canonical source of record nsids, record types, and per-record
// type guards is `./generated/records`, which `idiolect-codegen` emits
// from the lexicons. this module keeps the legacy `NSIDS` / `Nsid` /
// `RecordNsid` names working so existing call sites (and external
// consumers of `@idiolect/schema`) do not need to migrate in lockstep.

import { NSID, RECORD_NSIDS } from "./generated/records";

/**
 * Canonical NSIDs, re-exported under the pre-codegen name (`NSIDS`).
 *
 * Prefer the singular `NSID` export from `./generated/records` in new
 * code; this alias stays to avoid breaking call sites.
 */
export const NSIDS = NSID;

/** Every dev.idiolect.* nsid, as a string-literal union. */
export type Nsid = (typeof NSIDS)[keyof typeof NSIDS];

/** Every dev.idiolect.* record nsid (excludes the shared defs lexicon). */
export type RecordNsid = (typeof RECORD_NSIDS)[number];

export { RECORD_NSIDS };
