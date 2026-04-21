import type { Lexicons, ValidationResult } from "@atproto/lexicon";
import { defaultLexicons } from "./lexicons";
import { RECORD_NSIDS, type RecordNsid } from "./nsids";
import type { RecordMap } from "./recordMap";

/**
 * Validate a candidate record against a given nsid using a `Lexicons`
 * instance. Returns the at-proto validation result.
 *
 * @param nsid - the lexicon nsid to validate against.
 * @param value - the candidate record.
 * @param lex - optional lexicons instance; defaults to the shipped set.
 */
export function validateRecord<N extends RecordNsid>(
  nsid: N,
  value: unknown,
  lex: Lexicons = defaultLexicons(),
): ValidationResult<RecordMap[N]> {
  return lex.validate(nsid, value) as ValidationResult<RecordMap[N]>;
}

/**
 * Type-narrowing guard. Returns true iff `value` validates against
 * the lexicon identified by `nsid`.
 */
export function isRecord<N extends RecordNsid>(
  nsid: N,
  value: unknown,
  lex: Lexicons = defaultLexicons(),
): value is RecordMap[N] {
  return validateRecord(nsid, value, lex).success;
}

/**
 * Validate that `value` is a record for any of the dev.idiolect.*
 * record lexicons. Used by ingest pipelines that accept mixed traffic.
 *
 * Returns the matching nsid on success, or null if no schema accepts
 * the record.
 */
export function classifyRecord(
  value: unknown,
  lex: Lexicons = defaultLexicons(),
): RecordNsid | null {
  for (const nsid of RECORD_NSIDS) {
    if (validateRecord(nsid, value, lex).success) {
      return nsid;
    }
  }
  return null;
}
