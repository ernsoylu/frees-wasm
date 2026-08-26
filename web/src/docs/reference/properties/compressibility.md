---
name: compressibility
category: Fluid Properties
summary: Fluid property: compressibility from the real-fluid property backend.
related: []
examples: [thermo-compliance]
tags: [compressibility, property, fluid, coolprop]
references: []
---

# compressibility

Returns the **compressibility** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
compressibility(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
