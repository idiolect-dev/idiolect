# Attitudinal compositions for dev.idiolect.* records.
#
# Each record is a panproto theory merge: ThAtt supplies the
# holder/content backbone, an attitudinal flavour (assertive,
# doxastic, endorsive, bouletic, commissive, declarative) supplies
# the stance, and an idiolect content theory supplies the subject
# matter. Every step names the shared_sorts and shared_ops that
# the colimit identifies so the pushout is reproducible rather than
# order-dependent.
{
  # encounter: an assertion that a structured use was performed,
  # optionally carrying evidential grounding when the record is a
  # third-party attestation.
  encounter = {
    compose = {
      result = "ThAssertedUse",
      bases = ["ThAtt", "ThAssertive", "ThUse", "ThEvidential"],
      steps = [
        {
          left = "ThAtt",
          right = "ThAssertive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThUse",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_1",
          right = "ThEvidential",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # recommendation: an endorsement of a lens path under structured
  # applicability conditions (ThPredicate + ThCondition — the
  # boolean combinators come from ThPredicate, the context
  # predicates from ThCondition), with evidential grounding for
  # the endorsement.
  recommendation = {
    compose = {
      result = "ThConditionalEndorsement",
      bases = [
        "ThAtt",
        "ThEndorsive",
        "ThLensPath",
        "ThPredicate",
        "ThCondition",
        "ThEvidential",
      ],
      steps = [
        {
          left = "ThAtt",
          right = "ThEndorsive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThLensPath",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_1",
          right = "ThPredicate",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_2",
          right = "ThCondition",
          shared_sorts = ["Holder", "Content", "Att", "Predicate", "Subject"],
          shared_ops = ["holder", "content", "holds", "and_", "or_", "not_"],
        },
        {
          left = "step_3",
          right = "ThEvidential",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # verification: a doxastic claim about a specific lens property.
  verification = {
    compose = {
      result = "ThVerificationClaim",
      bases = ["ThAtt", "ThDoxastic", "ThLensProperty", "ThEvidential"],
      steps = [
        {
          left = "ThAtt",
          right = "ThDoxastic",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThLensProperty",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_1",
          right = "ThEvidential",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # bounty: a bouletic request paired with a commissive offer plus
  # structured constraints and eligibility.
  bounty = {
    compose = {
      result = "ThBoundedRequest",
      bases = [
        "ThAtt",
        "ThBouletic",
        "ThCommissive",
        "ThDeliverable",
        "ThConstraint",
        "ThPredicate",
        "ThEligibility",
        "ThEvidential",
      ],
      steps = [
        {
          left = "ThAtt",
          right = "ThBouletic",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThCommissive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_1",
          right = "ThDeliverable",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_2",
          right = "ThConstraint",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_3",
          right = "ThPredicate",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_4",
          right = "ThEligibility",
          shared_sorts = ["Holder", "Content", "Att", "Predicate", "Subject"],
          shared_ops = ["holder", "content", "holds", "and_", "or_", "not_"],
        },
        {
          left = "step_5",
          right = "ThEvidential",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # correction: an assertion of an edit attributed by reason.
  correction = {
    compose = {
      result = "ThAttributedCorrection",
      bases = ["ThAtt", "ThAssertive", "ThEdit", "ThEvidential"],
      steps = [
        {
          left = "ThAtt",
          right = "ThAssertive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThEdit",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_1",
          right = "ThEvidential",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # retrospection: a delayed assertion of a causally attributed
  # finding. ThDelay captures the gap between the finding's
  # occurrence and its detection.
  retrospection = {
    compose = {
      result = "ThDelayedFinding",
      bases = ["ThAtt", "ThAssertive", "ThFinding", "ThDelay", "ThEvidential"],
      steps = [
        {
          left = "ThAtt",
          right = "ThAssertive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThFinding",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_1",
          right = "ThDelay",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_2",
          right = "ThEvidential",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # observation: an aggregate assertion over a scope. Method and
  # output shapes stay polymorphic.
  observation = {
    compose = {
      result = "ThAggregateObservation",
      bases = ["ThAtt", "ThAssertive", "ThEvidential"],
      steps = [
        {
          left = "ThAtt",
          right = "ThAssertive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThEvidential",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # adapter: an assertion about wrapper behaviour — ability-typed
  # because adapters claim a capacity to wrap their framework.
  adapter = {
    compose = {
      result = "ThAdapterClaim",
      bases = ["ThAtt", "ThAssertive", "ThAbility"],
      steps = [
        {
          left = "ThAtt",
          right = "ThAssertive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThAbility",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # community: the self-constitutive declaration that a group exists
  # with these canonical schemas, lenses, and conventions.
  community = {
    compose = {
      result = "ThConstitutiveCharter",
      bases = ["ThAtt", "ThDeclarative", "ThCharter"],
      steps = [
        {
          left = "ThAtt",
          right = "ThDeclarative",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThCharter",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # dialect: the owning community's declarative endorsement of a
  # preferred idiolect bundle + preferred lenses.
  dialect = {
    compose = {
      result = "ThDialectCharter",
      bases = ["ThAtt", "ThDeclarative", "ThCharter"],
      steps = [
        {
          left = "ThAtt",
          right = "ThDeclarative",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThCharter",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # belief: a doxastic claim about another record. Generic over
  # subject record kind — the strong-ref in the subject field pins
  # what the belief is about; ThDoxastic + ThEvidential supply the
  # stance and grounding.
  belief = {
    compose = {
      result = "ThNestedBelief",
      bases = ["ThAtt", "ThDoxastic", "ThEvidential"],
      steps = [
        {
          left = "ThAtt",
          right = "ThDoxastic",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThEvidential",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },
}
