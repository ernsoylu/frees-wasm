---
name: soundspeed
category: Fluid Properties
summary: Fluid property: soundspeed from the real-fluid property backend.
related: []
examples: []
tags: [soundspeed, property, fluid, coolprop]
references: []
---

# soundspeed

Returns the **soundspeed** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
soundspeed(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
