---
name: viscosity
category: Fluid Properties
summary: Fluid property: viscosity from a real-fluid (CoolProp) backend.
related: []
examples: []
tags: [viscosity, property, fluid, coolprop]
references: []
---

# viscosity

Returns the **viscosity** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
viscosity(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
