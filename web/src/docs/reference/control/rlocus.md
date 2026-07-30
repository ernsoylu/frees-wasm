---
name: rlocus
category: Control Systems
summary: Root-locus trajectories of the closed-loop poles as gain K varies.
related: [pole, margin, place]
examples: [root-locus-analysis]
tags: [control, root locus, poles, gain, design, stability]
---

# rlocus

Returns the **root-locus** of `G(s) = num/den` — the paths the closed-loop poles
trace in the s-plane as the loop gain `K` sweeps from 0 to ∞. Use it to choose a
gain that places the dominant poles for a target damping or settling time.

## Syntax

```
CALL rlocus(num, den : K, cpr, cpi)
[K, cpr, cpi] = rlocus(num, den)
```

## Description

For unity feedback `1 + K·G(s) = 0`, the roots move from the open-loop poles
(`K = 0`) toward the open-loop zeros and asymptotes (`K → ∞`). `cpr`/`cpi` are the
real/imaginary parts of the closed-loop poles at each gain `K`.

## Mathematical Formulation

The locus is the set of `s` satisfying the characteristic equation

$$ 1 + K\,G(s) = 0 \quad\Longleftrightarrow\quad \angle G(s) = \pm 180°(2\ell+1) $$

with the gain at any locus point `K = 1/|G(s)|`.

> **Method:** sweep `K`, solving the characteristic polynomial roots at each value.

## Examples

### Example 1 — Root locus of a plant

[Run: root-locus-analysis]

**Expected:** branches leaving the open-loop poles and ending on the zeros /
asymptotes; crossings of the imaginary axis mark the stability-limiting gain.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Open-loop numerator (descending powers of `s`). |
| `den` | Vector | Yes | Open-loop denominator. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `K` | Vector | Gain values along the locus. |
| `cpr` | Vector/Matrix | Real parts of the closed-loop poles. |
| `cpi` | Vector/Matrix | Imaginary parts of the closed-loop poles. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `EMPTY_DENOMINATOR` | invalid `den` | Provide a valid open-loop denominator. |
