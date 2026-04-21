import { describe, expect, test } from "bun:test";
import {
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
} from "../src/examples.ts";
import { classifyRecord, validateRecord } from "../src/validators.ts";

// Each bundled fixture must validate against its own lexicon. If the
// generated code drifts away from the lexicon json the test fails
// immediately — the parsed-fixture module is the round-trip witness.

describe("parsed fixtures round-trip through validators", () => {
  const cases: Array<[string, unknown, string]> = [
    ["dev.idiolect.adapter", adapterExample, "adapterExample"],
    ["dev.idiolect.bounty", bountyExample, "bountyExample"],
    ["dev.idiolect.community", communityExample, "communityExample"],
    ["dev.idiolect.correction", correctionExample, "correctionExample"],
    ["dev.idiolect.dialect", dialectExample, "dialectExample"],
    ["dev.idiolect.encounter", encounterExample, "encounterExample"],
    ["dev.idiolect.observation", observationExample, "observationExample"],
    ["dev.idiolect.recommendation", recommendationExample, "recommendationExample"],
    ["dev.idiolect.retrospection", retrospectionExample, "retrospectionExample"],
    ["dev.idiolect.verification", verificationExample, "verificationExample"],
  ];

  for (const [nsid, fixture, label] of cases) {
    test(`${label} validates against ${nsid}`, () => {
      const result = validateRecord(nsid, fixture);
      expect(result.success).toBe(true);
    });
  }
});

describe("examplesByNsid map", () => {
  test("contains every first-party record kind", () => {
    const keys = Object.keys(examplesByNsid).sort();
    expect(keys).toEqual([
      "dev.idiolect.adapter",
      "dev.idiolect.bounty",
      "dev.idiolect.community",
      "dev.idiolect.correction",
      "dev.idiolect.dialect",
      "dev.idiolect.encounter",
      "dev.idiolect.observation",
      "dev.idiolect.recommendation",
      "dev.idiolect.retrospection",
      "dev.idiolect.verification",
    ]);
  });

  test("every entry classifies against its own nsid", () => {
    for (const [nsid, record] of Object.entries(examplesByNsid)) {
      expect(classifyRecord(record)).toBe(nsid);
    }
  });
});
