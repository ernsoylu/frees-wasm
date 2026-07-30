---
name: prandtl
category: Fluid Properties
summary: Fluid property: prandtl from a real-fluid (CoolProp) backend.
related: []
examples: []
tags: [prandtl, property, fluid, coolprop]
references: []
---

# prandtl

Returns the **prandtl** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
prandtl(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
