---
name: intenergy
category: Fluid Properties
summary: Fluid property: intenergy from the real-fluid property backend.
related: []
examples: []
tags: [intenergy, property, fluid, coolprop]
references: []
---

# intenergy

Returns the **intenergy** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
intenergy(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
