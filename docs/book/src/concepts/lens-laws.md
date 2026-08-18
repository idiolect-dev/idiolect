# Lens semantics and laws

idiolect runs panproto 0.70.1's state-based asymmetric lenses. The basic idea is
to retain whatever a target view cannot express, then use that retained state
when translating backward. We call this retained state the
[complement](../glossary.md#complement "State retained so a backward lens operation can reconstruct its source").

## State-based form

For a source space $S$, view space $V$, and complement space $C$, the runtime
shape is:

$$
\get : S \to V \times C
$$

$$
\put : V \times C \to S
$$

Given a source $s$, `get` returns a view $v$ and complement $c$. A caller may
modify the view and then call `put` with the modified view and the original
complement. In `idiolect-lens`, `apply_lens` and `apply_lens_put` expose these
two directions over JSON records after parsing them into panproto instances.

Consider the event schemas from [Why idiolect exists](./why-idiolect.md). If the
target has a structured `venue` but cannot represent every character of the
source's free-text `where`, `get` may place the residual source data in $c$.
The complement is thus record-specific state, not metadata that can be safely
reconstructed from the lens definition alone.

## Round-trip laws

A well-behaved lens satisfies two obligations. **GetPut** says that reading an
unmodified view and writing it back recovers the source:

$$
\put(\get(s)) = s
$$

Here `put(get(s))` abbreviates destructuring the pair returned by `get` and
passing both components to `put`.

**PutGet** says that writing a view with a compatible complement and reading it
again recovers that view:

$$
\pi_V\bigl(\get(\put(v,c))\bigr) = v
$$

The projection $\pi_V$ selects the view component. panproto's `check_laws`
checks GetPut on one concrete source, checks PutGet on its original view, and
also tries a mechanically modified view when one can be produced. Passing this
check is evidence about those instances; it is not a proof over all $s$, $v$,
and $c$.

For an isomorphism the complement is empty, and the two operations are
inverses:

$$
\put(\get(s)) = s
\qquad
\get(\put(v)) = v
$$

## Optic classification

panproto classifies a theory transform structurally with `OpticKind`. Version
0.70.1 uses these five variants:

| Kind | Structural reading | Complement role |
| --- | --- | --- |
| `Iso` | Bijection | Empty |
| `Lens` | Single-focus projection or extension | Retains dropped data or required defaults |
| `Prism` | Variant injection | Retains a variant tag |
| `Affine` | Composition of lens-like and prism-like behavior | Retains both forms of state |
| `Traversal` | Multi-focus transform | Tracks focus positions |

`classify_transform` assigns this kind from transform structure. Elementary
transforms are intended to be lawful by construction, but classification does
not itself run the laws. `check_optic_laws` performs the instance-level checks
available for the classified kind.

Composition uses the optic lattice implemented by `OpticKind::compose`: `Iso`
is the identity, `Traversal` absorbs the other kinds, and composing `Lens` with
`Prism` yields `Affine`. Concrete lens composition is sequential and must align
the first lens's target schema with the second lens's source schema.

## Coercion classes

Primitive value conversions have a separate `CoercionClass`. The class records
what relationship the forward and inverse functions claim:

| Class | Claim |
| --- | --- |
| `Iso` | Both round trips are identities. |
| `Retraction` | The inverse recovers every value in the forward image. |
| `Projection` | The target is deterministically derived from source data, but no inverse recovers the source from that target alone. |
| `Opaque` | No stronger structural relationship is claimed; the complement retains the original value. |

These classes compose differently from optic kinds. `Iso` is the identity,
`Opaque` absorbs, and composing a `Retraction` with a `Projection` collapses to
`Opaque`. panproto's sample-based coercion-law checker may falsify a declared
class, though a finite sample cannot establish a universal law.

## Symmetric lenses as spans

panproto builds a symmetric lens from two asymmetric lenses with a common
source schema $M$:

$$
\ell_L : M \to L \times C_L
$$

$$
\ell_R : M \to R \times C_R
$$

To synchronize a left view into a right view, the runtime first uses the left
leg's `put` to reconstruct a middle instance, then applies the right leg's
`get`. This is a span through shared state, rather than a direct lens whose
source is $L$.

`idiolect-lens::apply_lens_symmetric` resolves two lens records, requires equal
`sourceSchema` references, and constructs this span. Its JSON-level entry point
rebuilds the middle instance with `put_without_complement`. Thus, the incoming leg
must be isomorphic: a lossy leg that needs saved complement data is rejected.
Callers that hold such data can instead use panproto's complement-aware
`SymmetricLens` operations directly.

## Verification records

The verification Lexicon recognizes seven open-enum kinds, but the current
`idiolect-verify` crate implements four runners:

- `RoundtripTestRunner` checks forward-then-backward equality on a nonempty,
  caller-supplied corpus.
- `PropertyTestRunner` performs the same round trip on values from a
  caller-supplied generator and finite budget.
- `StaticCheckRunner` validates the source and target panproto schema graphs; it
  does not execute the lens.
- `CoercionLawRunner` delegates to a caller-supplied coercion-law client.

A result of `holds` records that the configured run found no counterexample.
The runner, corpus or generator, tool version, and publisher thus remain
part of the evidence. [Author a verification runner](../guide/verify.md) covers
the operational interface.
