---
name: conductivity
category: Fluid Properties
summary: Fluid property: conductivity from a real-fluid (CoolProp) backend.
related: []
examples: []
tags: [conductivity, property, fluid, coolprop]
references: []
---

# conductivity

Returns the **conductivity** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
conductivity(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
