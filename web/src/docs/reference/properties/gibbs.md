---
name: gibbs
category: Fluid Properties
summary: Fluid property: gibbs from the real-fluid property backend.
related: []
examples: []
tags: [gibbs, property, fluid, coolprop]
references: []
---

# gibbs

Returns the **gibbs** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
gibbs(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
