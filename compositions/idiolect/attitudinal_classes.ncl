# Named attitudinal compositions for vocabulary classification.
#
# Each entry is a composition of ThAtt with one or more attitudinal
# flavours. Vocabulary entries (in theories/idiolect/*-vocab.yaml)
# declare their attitudinal type by pointing at one of these names.
# Consumers can then dispatch on both the id's position in the
# subsumption hierarchy AND the attitudinal shape of the action /
# purpose.
{
  # An action performed and attested. Matches encounter records.
  asserted_use = {
    compose = {
      result = "ThAssertedUseClass",
      bases = ["ThAtt", "ThAssertive", "ThUse"],
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
      ],
    },
  },

  # An action the holder wants to perform. Matches bounty
  # `wants` when the requested output is a kind of action.
  desired_use = {
    compose = {
      result = "ThDesiredUseClass",
      bases = ["ThAtt", "ThBouletic", "ThUse"],
      steps = [
        {
          left = "ThAtt",
          right = "ThBouletic",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThUse",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # An action the holder intends to perform. Distinct from
  # desired_use (bouletic) — ThIntentional adds commitment.
  intended_use = {
    compose = {
      result = "ThIntendedUseClass",
      bases = ["ThAtt", "ThIntentional", "ThUse"],
      steps = [
        {
          left = "ThAtt",
          right = "ThIntentional",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThUse",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # An action the holder permits. Matches user-intent permissions
  # and community-level access grants.
  permitted_use = {
    compose = {
      result = "ThPermittedUseClass",
      bases = ["ThAtt", "ThPermissive", "ThUse"],
      steps = [
        {
          left = "ThAtt",
          right = "ThPermissive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThUse",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },

  # An action the holder prohibits. The dual of permitted_use.
  prohibited_use = {
    compose = {
      result = "ThProhibitedUseClass",
      bases = ["ThAtt", "ThProhibitive", "ThUse"],
      steps = [
        {
          left = "ThAtt",
          right = "ThProhibitive",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
        {
          left = "step_0",
          right = "ThUse",
          shared_sorts = ["Holder", "Content", "Att"],
          shared_ops = ["holder", "content"],
        },
      ],
    },
  },
}
