# Lens semantics and laws

A lens is a structured pair of functions between two schemas:

$$
\get : A \to (B, \complement) \qquad \put : (B, \complement) \to A
$$

`get` translates a source value `A` into a target view `B` plus a
**complement** $\complement$ (the data the projection discarded).
`put` takes a (possibly modified) `B` and the complement, and
reconstructs an `A`.

This shape is panproto's _state-based asymmetric lens_ and is
what `idiolect-lens` runs on the wire.

## The laws

A well-formed lens obeys two laws.

### GetPut

Restoring the complement recovers the original.

$$
\put(\get(a)) = a \qquad \forall a \in A
$$

In runtime form: if you `apply_lens` a record forward and then
`apply_lens_put` it back, you get the source bytes you started
with.

### PutGet

Lifting then projecting gives back the modified view.

$$
\get(\put(b, c)) = (b, c) \qquad \forall b \in B, c \in \complement
$$

In runtime form: if you write a target view through `put` and
then read it back through `get`, the view and complement match
what you wrote.

### Iso laws

When the lens is an isomorphism (no information dropped), the
complement is empty and the laws collapse:

$$
\put(\get(a)) = a \qquad \get(\put(b)) = b
$$

The two directions are total inverses.

## Optic classification

panproto's classifier assigns each lens chain one of five classes,
based on what the chain promises:

| Class | What it promises | What it allows |
| --- | --- | --- |
| **Iso** | Bijective; both directions are total inverses. | Auto-merge under the lexicon-evolution policy. |
| **Injection** | Source embeds in target without loss. Forward is total; backward needs no complement. | Auto-merge as forward-only. |
| **Projection** | Target is a quotient of source; forward drops information. Backward needs the complement. | PR review under the policy. |
| **Affine** | Partial. The forward direction may fail on some inputs. | PR review plus a community recommendation. |
| **General** | None of the above. | Manual lens authoring, full coercion-law check, plus verification. |

The class is not a quality judgment; it is a routing decision.
Some legitimate migrations are projections (a field genuinely went
away). The policy makes the consequences visible.

## Composition

Chain composition is associative:

$$
(\ell_1 \circ \ell_2) \circ \ell_3 \;=\; \ell_1 \circ (\ell_2 \circ \ell_3)
$$

Identity is the no-op lens; it is a left and right identity for
composition. panproto's protolens runtime auto-simplifies adjacent
steps where it can (`RenameVertex(a,b) ; RenameVertex(b,c) →
RenameVertex(a,c)`).

## Symmetric lenses

A symmetric lens pairs two state-based lenses that share a middle
schema:

$$
\ell_{ab} : A \to (M, \complement_a) \qquad \ell_{bc} : B \to (M, \complement_b)
$$

Sync from $A$ to $B$ goes $A \to M \to B$, threading the
complement through both halves. The dual sync $B \to A$ uses the
same machinery in reverse. `apply_lens_symmetric` runs either
direction.

This is the right shape for bridging two communities' lexicons:
each community owns its own lens to a shared middle schema, and
the bridge stays consistent up to complement.

## Coercion honesty

Lens chains that cross primitive kinds (Int↔Str, Float↔Int, ...)
declare a `CoercionClass` per kind crossing:

| `CoercionClass` | When |
| --- | --- |
| `Iso` | Forward and inverse are total inverses (e.g. `Int` to its decimal string and back). |
| `Retraction` | Forward is total; inverse recovers the forward image only. |
| `Projection` | Forward drops information (e.g. `Float` to `Int` by truncation). |
| `Opaque` | Documentation pair; no round-trip promise. |

A dishonest `Iso` declaration silently corrupts the GetPut law.
panproto ships a sample-based law checker
(`schema theory check-coercion-laws`) that catches violations.
The checker is wired into the lexicon-evolution gate.

## What you have to verify

Stating the laws is cheap. Verifying them on a corpus is the
work. The shipped runner kinds:

- `roundtrip-test` runs GetPut on a corpus.
- `property-test` runs an arbitrary boolean predicate.
- `static-check` runs the panproto-level coercion-law and
  existence checks against the chain itself.

A lens with no published verifications is a claim; a lens with
multiple published verifications from trusted signers, run on
recent corpora, is closer to an asserted fact. See
[Author a verification runner](../guide/verify.md) for the
authoring path.
