---
name: cv
category: Fluid Properties
summary: Fluid property: cv from a real-fluid (CoolProp) backend.
related: []
examples: []
tags: [cv, property, fluid, coolprop]
references: []
---

# cv

Returns the **cv** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
cv(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
