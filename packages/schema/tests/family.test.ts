import { describe, expect, test } from "bun:test";
import {
  decodeRecord,
  FAMILY_ID,
  FAMILY_NSID_PREFIX,
  familyContains,
  NSID,
  RECORD_NSIDS,
  toTypedJson,
  type AnyRecord,
  type Encounter,
} from "../src/generated/family.ts";
import { tagRecord } from "../src/generated/family.ts";

// Family identity tests are cheap pins on the codegen output. They
// also catch a regression where `idiolect_family()` gets re-anchored
// to a different family by mistake.
describe("family identity", () => {
  test("FAMILY_ID matches dev.idiolect", () => {
    expect(FAMILY_ID).toBe("dev.idiolect");
  });

  test("FAMILY_NSID_PREFIX matches dev.idiolect.", () => {
    expect(FAMILY_NSID_PREFIX).toBe("dev.idiolect.");
  });

  test("RECORD_NSIDS has every NSID and only NSIDs", () => {
    const nsidValues = new Set(Object.values(NSID));
    expect(new Set(RECORD_NSIDS)).toEqual(nsidValues);
  });
});

// Membership predicate tests. The predicate's narrowing claim
// (`nsid is NSID`) must hold at runtime: passing a string that
// merely shares the family's prefix but isn't a known record NSID
// must return false. Testing this guards against a regression to
// the prefix-only check.
describe("familyContains", () => {
  test("returns true for every member NSID", () => {
    for (const nsid of RECORD_NSIDS) {
      expect(familyContains(nsid)).toBe(true);
    }
  });

  test("returns false for a same-prefix non-member NSID", () => {
    expect(familyContains("dev.idiolect.notarealrecord")).toBe(false);
  });

  test("returns false for an empty string", () => {
    expect(familyContains("")).toBe(false);
  });

  test("returns false for an out-of-family NSID", () => {
    expect(familyContains("com.atproto.repo.createRecord")).toBe(false);
  });

  test("narrows the type predicate when true", () => {
    const candidate: string = NSID.encounter;
    if (familyContains(candidate)) {
      // `candidate` should narrow to `NSID` here. The compile-time
      // assertion is enforced by typecheck; the runtime check is
      // the membership equality below.
      expect(RECORD_NSIDS as readonly string[]).toContain(candidate);
    } else {
      throw new Error("familyContains should accept a known NSID");
    }
  });
});

// Decode + encode are inverses on the wire-form roundtrip. The
// loose `DecodedRecord` shape carries `body: unknown`, so the
// roundtrip equality test compares JSON shapes via deepEqual.

const ENCOUNTER: Encounter = {
  lens: { uri: "at://did:plc:example/dev.idiolect.lens/abc" },
  sourceSchema: { uri: "at://did:plc:example/dev.idiolect.schema/src" },
  use: { action: "translate_source_to_target" },
  kind: "invocation-log",
  visibility: "public-detailed",
  occurredAt: "2026-04-19T00:00:00.000Z",
};

describe("decodeRecord", () => {
  test("returns null for non-object inputs", () => {
    expect(decodeRecord(null)).toBeNull();
    expect(decodeRecord(undefined)).toBeNull();
    expect(decodeRecord(42)).toBeNull();
    expect(decodeRecord("string")).toBeNull();
    expect(decodeRecord([])).toBeNull();
  });

  test("returns null when $type is missing", () => {
    expect(decodeRecord({ kind: "invocation-log" })).toBeNull();
  });

  test("returns null when $type is not a string", () => {
    expect(decodeRecord({ $type: 7 })).toBeNull();
  });

  test("returns null when $type is out of family", () => {
    expect(
      decodeRecord({ $type: "com.atproto.repo.createRecord", foo: 1 }),
    ).toBeNull();
  });

  test("returns null for a same-prefix non-member $type", () => {
    expect(
      decodeRecord({ $type: "dev.idiolect.notarealrecord", foo: 1 }),
    ).toBeNull();
  });

  test("strips $type from the body and tags it on success", () => {
    const wire = { $type: NSID.encounter, ...ENCOUNTER };
    const decoded = decodeRecord(wire);
    expect(decoded).not.toBeNull();
    expect(decoded!.$nsid).toBe(NSID.encounter);
    // $type should not survive in the body.
    expect((decoded!.body as Record<string, unknown>)).not.toHaveProperty(
      "$type",
    );
    // Every other field roundtrips.
    expect(decoded!.body).toEqual({ ...ENCOUNTER });
  });
});

describe("toTypedJson", () => {
  test("inlines $nsid as $type alongside the value", () => {
    const tagged: AnyRecord = tagRecord(NSID.encounter, ENCOUNTER);
    const wire = toTypedJson(tagged);
    expect(wire["$type"]).toBe(NSID.encounter);
    // Every field of the value is in the wire form.
    for (const [k, v] of Object.entries(ENCOUNTER)) {
      expect(wire[k]).toEqual(v);
    }
  });

  test("decodeRecord ∘ toTypedJson preserves nsid and body", () => {
    const tagged: AnyRecord = tagRecord(NSID.encounter, ENCOUNTER);
    const wire = toTypedJson(tagged);
    const decoded = decodeRecord(wire);
    expect(decoded).not.toBeNull();
    expect(decoded!.$nsid).toBe(tagged.$nsid);
    expect(decoded!.body).toEqual({ ...tagged.value });
  });
});
