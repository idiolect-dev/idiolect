//! NSID constants for the `dev.idiolect.*` Lexicon family.

/// Shared-definitions Lexicon (not a record).
pub const DEFS: &str = "dev.idiolect.defs";

/// Community record NSID.
pub const COMMUNITY: &str = "dev.idiolect.community";

/// Dialect record NSID.
pub const DIALECT: &str = "dev.idiolect.dialect";

/// Encounter record NSID.
pub const ENCOUNTER: &str = "dev.idiolect.encounter";

/// Correction record NSID.
pub const CORRECTION: &str = "dev.idiolect.correction";

/// Verification record NSID.
pub const VERIFICATION: &str = "dev.idiolect.verification";

/// Observation record NSID.
pub const OBSERVATION: &str = "dev.idiolect.observation";

/// Retrospection record NSID.
pub const RETROSPECTION: &str = "dev.idiolect.retrospection";

/// Recommendation record NSID.
pub const RECOMMENDATION: &str = "dev.idiolect.recommendation";

/// Adapter record NSID.
pub const ADAPTER: &str = "dev.idiolect.adapter";

/// Bounty record NSID.
pub const BOUNTY: &str = "dev.idiolect.bounty";

/// The ten record NSIDs in declaration order.
pub const RECORDS: [&str; 10] = [
    COMMUNITY,
    DIALECT,
    ENCOUNTER,
    CORRECTION,
    VERIFICATION,
    OBSERVATION,
    RETROSPECTION,
    RECOMMENDATION,
    ADAPTER,
    BOUNTY,
];
