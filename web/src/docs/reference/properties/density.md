---
name: density
category: Fluid Properties
summary: Fluid property: density from the real-fluid property backend.
related: []
examples: []
tags: [density, property, fluid, coolprop]
references: []
---

# density

Returns the **density** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
density(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
