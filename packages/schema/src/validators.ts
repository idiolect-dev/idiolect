import type { Lexicons, ValidationResult } from "@atproto/lexicon";
import { NSID, type NSID as NsidType, RECORD_NSIDS, type RecordTypes } from "./generated/family";
import { defaultLexicons } from "./lexicons";

/**
 * Validate a candidate record against a given nsid using a `Lexicons`
 * instance. Returns the at-proto validation result.
 *
 * @param nsid - the lexicon nsid to validate against.
 * @param value - the candidate record.
 * @param lex - optional lexicons instance; defaults to the shipped set.
 */
export function validateRecord<N extends NsidType>(
  nsid: N,
  value: unknown,
  lex: Lexicons = defaultLexicons(),
): ValidationResult<RecordTypes[N]> {
  return lex.validate(nsid, value) as ValidationResult<RecordTypes[N]>;
}

/**
 * Type-narrowing guard. Returns true iff `value` validates against
 * the lexicon identified by `nsid`.
 */
export function isRecord<N extends NsidType>(
  nsid: N,
  value: unknown,
  lex: Lexicons = defaultLexicons(),
): value is RecordTypes[N] {
  return validateRecord(nsid, value, lex).success;
}

/**
 * Validate that `value` is a record for any of the family's record
 * lexicons. Used by ingest pipelines that accept mixed traffic.
 *
 * Returns the matching nsid on success, or null if no schema accepts
 * the record.
 */
export function classifyRecord(value: unknown, lex: Lexicons = defaultLexicons()): NsidType | null {
  for (const nsid of RECORD_NSIDS) {
    if (validateRecord(nsid, value, lex).success) {
      return nsid;
    }
  }
  return null;
}

// Re-export the canonical NSID const so call sites can do
// `validateRecord(NSID.encounter, ...)` without a separate import.
export { NSID };
