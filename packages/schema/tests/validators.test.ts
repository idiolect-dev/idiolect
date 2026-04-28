import { describe, expect, test } from "bun:test";
import type {
  Adapter,
  Bounty,
  Correction,
  Encounter,
  Observation,
  Retrospection,
  Verification,
} from "../src/generated/index.ts";
import { NSID, RECORD_NSIDS } from "../src/generated/family.ts";
import { classifyRecord, isRecord, validateRecord } from "../src/validators.ts";

// minimal valid examples used to assert that the generated types
// are consistent with the lexicon json. if drift appears in either
// direction these tests fail. per atproto convention the wire form
// carries `$type`, but the validator takes the nsid separately, so
// fixtures omit it here.

const ENCOUNTER: Encounter = {
  lens: { uri: "at://did:plc:example/dev.idiolect.lens/abc" },
  sourceSchema: { uri: "at://did:plc:example/dev.idiolect.schema/src" },
  use: { action: "translate_source_to_target" },
  kind: "invocation-log",
  visibility: "public-detailed",
  occurredAt: "2026-04-19T00:00:00.000Z",
};

const CORRECTION: Correction = {
  encounter: { uri: "at://did:plc:example/dev.idiolect.encounter/abc" },
  path: "/foo/bar",
  reason: "lens-error",
  visibility: "public-detailed",
  occurredAt: "2026-04-19T00:00:00.000Z",
};

const VERIFICATION: Verification = {
  lens: { uri: "at://did:plc:example/dev.idiolect.lens/abc" },
  kind: "roundtrip-test",
  verifier: "did:plc:verifier",
  tool: { name: "nextest", version: "0.9.87" },
  property: {
    $type: "dev.idiolect.defs#lpRoundtrip",
    domain: "all valid records",
  },
  result: "holds",
  occurredAt: "2026-04-19T00:00:00.000Z",
};

const OBSERVATION: Observation = {
  observer: "did:plc:observer",
  method: { name: "weighted-correction-rate" },
  scope: { encounterKinds: ["production"] },
  output: { lenses: [] },
  version: "1.0.0",
  visibility: "public-detailed",
  occurredAt: "2026-04-19T00:00:00.000Z",
};

const RETROSPECTION: Retrospection = {
  encounter: { uri: "at://did:plc:example/dev.idiolect.encounter/abc" },
  finding: { kind: "merge-divergence", detail: "left branch lost a record" },
  detectingParty: "did:plc:detector",
  detectedAt: "2026-04-19T06:00:00.000Z",
  occurredAt: "2026-04-19T06:30:00.000Z",
};

const ADAPTER: Adapter = {
  framework: "coq",
  versionRange: "^8.20",
  invocationProtocol: { kind: "subprocess" },
  isolation: { kind: "process" },
  author: "did:plc:adapter-author",
  occurredAt: "2026-04-19T00:00:00.000Z",
};

const BOUNTY: Bounty = {
  requester: "did:plc:requester",
  wants: {
    $type: "dev.idiolect.bounty#wantLens",
    source: { language: "postgres-sql" },
    target: { language: "atproto-lexicon" },
  },
  constraints: [],
  occurredAt: "2026-04-19T00:00:00.000Z",
};

describe("validateRecord", () => {
  test("accepts a minimal valid encounter", () => {
    expect(validateRecord(NSID.encounter, ENCOUNTER).success).toBe(true);
  });

  test("accepts every minimal example at its own nsid", () => {
    const samples: Array<[(typeof RECORD_NSIDS)[number], unknown]> = [
      [NSID.encounter, ENCOUNTER],
      [NSID.correction, CORRECTION],
      [NSID.verification, VERIFICATION],
      [NSID.observation, OBSERVATION],
      [NSID.retrospection, RETROSPECTION],
      [NSID.adapter, ADAPTER],
      [NSID.bounty, BOUNTY],
    ];
    for (const [nsid, value] of samples) {
      const result = validateRecord(nsid, value);
      expect(result.success, `${nsid} should validate`).toBe(true);
    }
  });

  test("rejects encounter missing required `use`", () => {
    const { use: _drop, ...bad } = ENCOUNTER;
    expect(validateRecord(NSID.encounter, bad).success).toBe(false);
  });

  test("rejects correction with unknown reason", () => {
    const bad = { ...CORRECTION, reason: "invented" };
    expect(validateRecord(NSID.correction, bad).success).toBe(false);
  });
});

describe("isRecord", () => {
  test("narrows to Encounter on success", () => {
    const maybe: unknown = ENCOUNTER;
    if (isRecord(NSID.encounter, maybe)) {
      expect(maybe.kind).toBe("invocation-log");
    } else {
      throw new Error("expected encounter to narrow");
    }
  });
});

describe("classifyRecord", () => {
  test("identifies a bounty record", () => {
    expect(classifyRecord(BOUNTY)).toBe(NSID.bounty);
  });

  test("returns null for a foreign payload", () => {
    expect(classifyRecord({ hello: "world" })).toBeNull();
  });
});
