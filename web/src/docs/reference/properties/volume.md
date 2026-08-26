---
name: volume
category: Fluid Properties
summary: Fluid property: volume from the real-fluid property backend.
related: []
examples: [rankine-cycle, thermo-compliance, rankine-cycle, engine-cycle-wiebe]
tags: [volume, property, fluid, coolprop]
references: []
---

# volume

Returns the **volume** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
volume(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
