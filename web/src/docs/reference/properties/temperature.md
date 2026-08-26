---
name: temperature
category: Fluid Properties
summary: Fluid property: temperature from the real-fluid property backend.
related: []
examples: [pressure-cooker]
tags: [temperature, property, fluid, coolprop]
references: []
---

# temperature

Returns the **temperature** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
temperature(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
