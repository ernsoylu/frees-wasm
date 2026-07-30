---
name: probability
category: Stats
summary: Probability that a normal variate falls in the interval [x1, x2].
related: [normalcdf, normalpdf, chi_square]
examples: []
tags: [probability, stats, normal, gaussian, interval, range]
---

# probability

Returns the probability that a normally distributed variable with mean `mu` and
standard deviation `sigma` falls between `x1` and `x2`. For a one-sided
cumulative probability `Pr(X ≤ x)` use `normalcdf(x, mu, sigma)` instead.

## Syntax

```
p = probability(x1, x2, mu, sigma)
```

## Description

Evaluates the area of the normal density between the two bounds. Use it for
"what fraction lies within these limits" questions — tolerances, pass/fail
bands, ±kσ coverage. Pass a very large/small bound to get a one-sided tail.

## Mathematical Formulation

$$ \Pr(x_1 \le X \le x_2) = \tfrac{1}{2}\left[\operatorname{erf}\!\left(\frac{x_2-\mu}{\sigma\sqrt{2}}\right) - \operatorname{erf}\!\left(\frac{x_1-\mu}{\sigma\sqrt{2}}\right)\right] $$

> **Method:** direct evaluation via the error function (Apache Commons Math `Erf.erf`).

## Examples

### Example 1 — Coverage within ±1σ

```
{ Fraction of a N(80, 5) population within one standard deviation }
p = probability(75, 85, 80, 5)   { 0.6827 }
```

**Expected:** `p ≈ 0.6827` — the classic 68% inside ±1σ.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x1` | Number | Yes | Lower bound of the interval. |
| `x2` | Number | Yes | Upper bound of the interval (`x2 ≥ x1`). |
| `mu` | Number | Yes | Mean of the normal distribution. |
| `sigma` | Number | Yes | Standard deviation (`> 0`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `p` | Number | Probability mass in `[x1, x2]`, in `[0, 1]`. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `Probability standard deviation must be > 0` | `sigma ≤ 0` | Pass a positive standard deviation. |
