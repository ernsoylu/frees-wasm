---
name: routh
category: Control Systems
summary: Routh-Hurwitz stability test — count of right-half-plane roots.
related: [pole, margin, rlocus]
examples: [routh-stability]
tags: [control, routh, hurwitz, stability, characteristic polynomial]
---

# routh

Applies the **Routh-Hurwitz criterion** to a characteristic polynomial `den(s)` and
returns the number of right-half-plane roots `nRHP` and a stability flag `stable`.
It decides stability without computing the roots — useful for symbolic gain ranges.

## Syntax

```
CALL routh(den : nRHP, stable)
[nRHP, stable] = routh(den)
```

## Description

The Routh array is built from the polynomial coefficients; the number of sign
changes in its first column equals the number of poles in the right half-plane. A
system is stable iff there are none.

## Mathematical Formulation

For `den(s) = a_n s^n + … + a_0`, the Routh array's first-column sign changes count
the RHP roots; stability requires:

$$ \text{all first-column entries} > 0 \quad\Longleftrightarrow\quad n_{RHP} = 0 $$

> **Method:** construct the Routh array (handling zero-pivot and zero-row special
> cases) and count first-column sign changes.

## Examples

### Example 1 — Stability of a characteristic polynomial

[Run: routh-stability]

**Expected:** `nRHP` right-half-plane roots and `stable = 1` only when `nRHP = 0`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `den` | Vector | Yes | Characteristic-polynomial coefficients (descending powers of `s`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `nRHP` | Number | Count of right-half-plane roots. |
| `stable` | Number | 1 if stable (`nRHP = 0`), else 0. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `EMPTY_DENOMINATOR` | invalid `den` | Provide a valid characteristic polynomial. |
