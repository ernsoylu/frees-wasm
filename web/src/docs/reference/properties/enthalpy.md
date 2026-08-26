---
name: enthalpy
category: Fluid Properties
summary: Fluid property: enthalpy from the real-fluid property backend.
related: []
examples: [rankine-cycle, state-tables-multifluid, rankine-cycle, refrigeration-vcr]
tags: [enthalpy, property, fluid, coolprop]
references: []
---

# enthalpy

Returns the **enthalpy** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
enthalpy(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
