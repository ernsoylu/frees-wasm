---
name: FunctionName                      # exact case as written in code (case-insensitive at runtime)
category: Category Name                  # must match a FunctionRegistry category
summary: One-line description of what it returns.
related: [RelatedFn1, RelatedFn2]        # See Also cross-links (names of other pages)
examples: [example-id-1, example-id-2]   # ids that exist in frontend/src/examples.ts (backend-verified)
tags: [keyword, keyword]                 # extra search terms
references:                              # optional; cite only public standards / data sources
  - "Public standard or data source, §section / Eq. (n) — what it grounds"
---

# FunctionName

One or two sentences: what it computes and the single most common reason to reach for it.

## Syntax

```
y = FunctionName(arg1, arg2)
y = FunctionName(arg1, arg2, "option")
```

## Description

What it does and when to use it (1–3 short paragraphs). Lead with behavior, not internals.

## Mathematical Formulation

State the governing equation(s) in KaTeX. Then one line on
the numerical method the backend actually uses.

$$ y = f(x) $$

> **Method:** <e.g. LU with partial pivoting / Brent root-find / real-fluid EOS inversion>.

## Examples

### Example 1 — <basic, with engineering context>

[Run: example-id-1]

**Expected:** `<key result, e.g. Q = 38.2 kW>`

### Example 2 — <intermediate/advanced, different domain>

[Run: example-id-2]

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `arg1` | Number | Yes | … (units / dims / constraints) |
| `"option"` | String | No | … (allowed values, default) |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | … |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `ERROR_CODE` | … | … |

## References

1. Public standard or data source, §section, Eq. (n).
