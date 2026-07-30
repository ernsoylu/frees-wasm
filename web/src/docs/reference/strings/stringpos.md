---
name: stringpos
category: Strings
summary: 1-based position of substring sub$ in s$ (0 if absent)
related: []
examples: []
tags: [stringpos, strings]
references: []
---

# stringpos

1-based position of substring sub$ in s$ (0 if absent)


## Syntax

```
StringPos(s$, sub$)
```

## Description

1-based position of substring sub$ in s$ (0 if absent)

## Mathematical Formulation

$$ \operatorname{StringPos}(s, t) = \text{1-based index of } t \text{ in } s,\ 0 \text{ if absent} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `s$` | String | Yes | String literal. |
| `sub$` | String | Yes | Substring to search for. |
