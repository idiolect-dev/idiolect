# Attitudinal compositions for dev.idiolect.* records.
#
# Each record is a panproto theory merge: ThAtt supplies the
# holder/content backbone, an attitudinal flavour (assertive, doxastic,
# endorsive, bouletic, commissive) supplies the stance, and an
# idiolect content theory supplies the subject matter.
#
# The attitudinal flavours are shared with dev.attitudes.*; that's the
# whole point — the attitudes framework and the idiolect framework
# compose from the same primitives.
{
  # encounter: an assertion that a purposeful invocation happened.
  encounter = {
    bases = ["ThAtt", "ThAssertive", "ThPurpose"],
    result = "ThPurposefulInvocation",
  },

  # recommendation: an endorsement of a lens path under conditions.
  recommendation = {
    bases = ["ThAtt", "ThEndorsive", "ThLensPath", "ThCondition"],
    result = "ThConditionalEndorsement",
  },

  # verification: a doxastic claim about a specific lens property.
  verification = {
    bases = ["ThAtt", "ThDoxastic", "ThLensProperty"],
    result = "ThVerificationClaim",
  },

  # bounty: the only record that pairs two attitudinal primitives —
  # bouletic ("I want a thing") with commissive ("and I commit to
  # these terms"). The pairing is inherent to what a bounty is.
  bounty = {
    bases = [
      "ThAtt",
      "ThBouletic",
      "ThCommissive",
      "ThDeliverable",
      "ThConstraint",
      "ThEligibility",
    ],
    result = "ThBoundedRequest",
  },

  # correction: an assertion of an edit attributed by reason.
  correction = {
    bases = ["ThAtt", "ThAssertive", "ThEdit"],
    result = "ThAttributedCorrection",
  },

  # retrospection: a delayed assertion of a causally attributed
  # finding. ThDelay captures the gap between occurred_at and the
  # event the finding is about.
  retrospection = {
    bases = ["ThAtt", "ThAssertive", "ThFinding", "ThDelay"],
    result = "ThDelayedFinding",
  },

  # observation: an aggregate assertion over a scope. Method and
  # output shapes stay polymorphic; the composition captures the
  # attitudinal stance, not the method-specific structure.
  observation = {
    bases = ["ThAtt", "ThAssertive"],
    result = "ThAggregateObservation",
  },

  # adapter: an assertion about wrapper behaviour — an adapter record
  # says "this wrapper does what we claim".
  adapter = {
    bases = ["ThAtt", "ThAssertive"],
    result = "ThAdapterClaim",
  },

  # community: the self-constitutive declaration that a group exists
  # with these canonical schemas, lenses, and conventions.
  # ThDeclarative (shared with dev.attitudes.*) plus ThCharter gives
  # the "this is what we are" stance.
  community = {
    bases = ["ThAtt", "ThDeclarative", "ThCharter"],
    result = "ThConstitutiveCharter",
  },

  # dialect: the owning community's declarative endorsement of a
  # preferred idiolect bundle + preferred lenses. Same attitudinal
  # primitive as community; ThCharter.endorsed_bundle is the load-
  # bearing op.
  dialect = {
    bases = ["ThAtt", "ThDeclarative", "ThCharter"],
    result = "ThDialectCharter",
  },
}
