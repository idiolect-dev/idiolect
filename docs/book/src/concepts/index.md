# Concepts

The Concepts chapters explain the model behind idiolect. They begin with a
concrete coordination failure, introduce the records that make translation
knowledge public, and then develop the formal and governance consequences.
Procedures stay in the [Guides](../guide/index.md); field-level contracts stay
in [Reference](../reference/index.md).

## A first pass

Read these chapters in order if the project is new to you:

1. [Why idiolect exists](./why-idiolect.md) names the private-converter problem
   and follows one translation through the record family.
2. [What you need first](./prerequisites.md) supplies the small amount of
   [ATProto](https://atproto.com/guides/overview) and panproto background used
   elsewhere.
3. [Idiolect, dialect, language](./idiolect-dialect-language.md) separates the
   linguistic analogy from the concrete runtime artifacts.
4. [The `dev.idiolect.*` lexicon family](./lexicon-family.md) maps those artifacts
   onto the sixteen record kinds shipped by the repository.

## Runtime model

The next four chapters explain how the runtime interprets those records:

- [Records in signed, content-addressed repositories](./atproto-records.md)
  distinguishes record identity, record content, and repository proof.
- [Open enums and vocabularies](./open-enums.md) explains how unfamiliar slugs
  survive decoding and acquire community-specific meanings.
- [The vocabulary knowledge graph](./vocab-graph.md) develops the graph queries
  behind that interpretation.
- [Observer protocol](./observer.md) explains how event folds become
  independently published observation records.

## Formal and governance paths

[Lens semantics and laws](./lens-laws.md) is the formal center of the book. It
introduces complements, round-trip laws, optic classification, and symmetric
span construction against panproto 0.71.0. [Deliberation](./deliberation.md)
then separates a community's decision process from its settled beliefs and
recommendations. [Lexicon evolution policy](./lexicon-evolution.md) closes the
section by comparing the intended migration gate with the enforcement that the
current checkout actually provides.
