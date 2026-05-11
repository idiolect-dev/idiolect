# Concepts

The Concepts section explains the model the runtime is built on.
The chapters are self-contained but assume basic familiarity with
ATProto records and panproto's lens vocabulary.

| Chapter | What it explains |
| --- | --- |
| [Idiolect, dialect, language](./idiolect-dialect-language.md) | The frame the project is named after; what each layer is responsible for. |
| [The dev.idiolect.* lexicon family](./lexicon-family.md) | The shipped lexicons, what each one names, and how they compose. |
| [Records as content-addressed signed data](./atproto-records.md) | Why ATProto's record model is the substrate; what the runtime gets for free. |
| [Lens semantics and laws](./lens-laws.md) | The `get` / `put` / complement model, GetPut, PutGet, optic classification. |
| [Open enums and vocabularies](./open-enums.md) | Why every enum field is open; how `*Vocab` siblings extend slugs. |
| [The vocabulary knowledge graph](./vocab-graph.md) | The typed multi-relation graph, OWL Lite, SKOS Core, registry queries. |
| [Deliberation](./deliberation.md) | The deliberation lexicons, how they relate to belief / recommendation. |
| [Observer protocol](./observer.md) | Why aggregate state lives in records, not in a central endpoint. |
| [Lexicon evolution policy](./lexicon-evolution.md) | Every lexicon revision ships with a derived, classified, verified, published lens. |
