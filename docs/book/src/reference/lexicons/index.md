# Lexicons

The `dev.idiolect.*` lexicon family. Every record kind that travels
on the network has a lexicon document under
`lexicons/dev/idiolect/`.

| NSID | Page |
| --- | --- |
| `dev.idiolect.adapter` | [adapter](./adapter.md) |
| `dev.idiolect.belief` | [belief](./belief.md) |
| `dev.idiolect.bounty` | [bounty](./bounty.md) |
| `dev.idiolect.community` | [community](./community.md) |
| `dev.idiolect.correction` | [correction](./correction.md) |
| `dev.idiolect.defs` | [defs](./defs.md) |
| `dev.idiolect.deliberation` | [deliberation](./deliberation.md) |
| `dev.idiolect.deliberationStatement` | [deliberationStatement](./deliberationStatement.md) |
| `dev.idiolect.deliberationVote` | [deliberationVote](./deliberationVote.md) |
| `dev.idiolect.deliberationOutcome` | [deliberationOutcome](./deliberationOutcome.md) |
| `dev.idiolect.dialect` | [dialect](./dialect.md) |
| `dev.idiolect.encounter` | [encounter](./encounter.md) |
| `dev.idiolect.observation` | [observation](./observation.md) |
| `dev.idiolect.recommendation` | [recommendation](./recommendation.md) |
| `dev.idiolect.retrospection` | [retrospection](./retrospection.md) |
| `dev.idiolect.verification` | [verification](./verification.md) |
| `dev.idiolect.vocab` | [vocab](./vocab.md) |

## Policy

The pages in this section are navigation summaries. The
authoritative shape for every lexicon is the JSON document under
`lexicons/dev/idiolect/<name>.json`; the generated Rust types on
[docs.rs/idiolect-records](https://docs.rs/idiolect-records/latest/idiolect_records/)
are derived from that JSON and are the authoritative typed
surface. When this book and either source disagree, the source
wins; please file an issue.
