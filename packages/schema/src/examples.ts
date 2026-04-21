// Typed parsed fixture records.
//
// The generated `examples.ts` module ships raw json strings
// (`AdapterJson`, `BountyJson`, …). This module parses them once at
// import time and exports strongly-typed record values so downstream
// code can use a fixture without reaching for `JSON.parse` and a
// cast.
//
// Mirrors the Rust side's `idiolect_records::generated::examples`
// module, which exposes parsed values via serde. The shape is
// deliberately narrow — only the first-party `dev.idiolect.*`
// records, which is where fixtures actually live.

import {
  AdapterJson,
  BountyJson,
  CommunityJson,
  CorrectionJson,
  DialectJson,
  EncounterJson,
  ObservationJson,
  RecommendationJson,
  RetrospectionJson,
  VerificationJson,
} from "./generated/examples";
import type {
  Adapter,
  Bounty,
  Community,
  Correction,
  Dialect,
  Encounter,
  Observation,
  Recommendation,
  Retrospection,
  Verification,
} from "./generated/index";

/** Parsed `dev.idiolect.adapter` fixture. */
export const adapterExample: Adapter = JSON.parse(AdapterJson) as Adapter;

/** Parsed `dev.idiolect.bounty` fixture. */
export const bountyExample: Bounty = JSON.parse(BountyJson) as Bounty;

/** Parsed `dev.idiolect.community` fixture. */
export const communityExample: Community = JSON.parse(CommunityJson) as Community;

/** Parsed `dev.idiolect.correction` fixture. */
export const correctionExample: Correction = JSON.parse(CorrectionJson) as Correction;

/** Parsed `dev.idiolect.dialect` fixture. */
export const dialectExample: Dialect = JSON.parse(DialectJson) as Dialect;

/** Parsed `dev.idiolect.encounter` fixture. */
export const encounterExample: Encounter = JSON.parse(EncounterJson) as Encounter;

/** Parsed `dev.idiolect.observation` fixture. */
export const observationExample: Observation = JSON.parse(ObservationJson) as Observation;

/** Parsed `dev.idiolect.recommendation` fixture. */
export const recommendationExample: Recommendation = JSON.parse(
  RecommendationJson,
) as Recommendation;

/** Parsed `dev.idiolect.retrospection` fixture. */
export const retrospectionExample: Retrospection = JSON.parse(RetrospectionJson) as Retrospection;

/** Parsed `dev.idiolect.verification` fixture. */
export const verificationExample: Verification = JSON.parse(VerificationJson) as Verification;

/**
 * Every first-party fixture grouped by nsid.
 *
 * Useful for driving lexicon-validator test sweeps:
 * `for (const [nsid, record] of Object.entries(examplesByNsid)) …`.
 */
export const examplesByNsid: Record<string, unknown> = {
  "dev.idiolect.adapter": adapterExample,
  "dev.idiolect.bounty": bountyExample,
  "dev.idiolect.community": communityExample,
  "dev.idiolect.correction": correctionExample,
  "dev.idiolect.dialect": dialectExample,
  "dev.idiolect.encounter": encounterExample,
  "dev.idiolect.observation": observationExample,
  "dev.idiolect.recommendation": recommendationExample,
  "dev.idiolect.retrospection": retrospectionExample,
  "dev.idiolect.verification": verificationExample,
};
