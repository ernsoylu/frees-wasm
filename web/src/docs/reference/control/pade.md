---
name: pade
category: Control Systems
summary: Padé rational approximation of a pure time delay.
related: [tf, series, feedback]
examples: []
tags: [control, pade, time delay, dead time, approximation]
---

# pade

Returns a **Padé rational approximation** `num/den` of a pure time delay
`e^{−Td·s}` of the given order. It replaces the transcendental delay with a rational
transfer function so the loop can be analyzed and designed with standard tools.

## Syntax

```
CALL pade(Td, order : num, den)
[num, den] = pade(Td, order)
```

## Mathematical Formulation

The order-`n` Padé approximant of the delay:

$$ e^{-T_d s} \approx \frac{N_n(-T_d s)}{N_n(T_d s)}, \qquad \text{e.g. (n=1): } \frac{1 - T_d s/2}{1 + T_d s/2} $$

with the right-half-plane zeros that give a delay its characteristic phase lag.

> **Method:** form the order-`n` Padé numerator/denominator polynomials in `Td·s`.

## Examples

```
{ [num, den] = pade(0.2, 2) approximates a 0.2 s delay to 2nd order }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Td` | Number | Yes | Time delay [s]. |
| `order` | Number | Yes | Approximation order (≥ 1). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `num` | Vector | Numerator of the approximant. |
| `den` | Vector | Denominator of the approximant. |
