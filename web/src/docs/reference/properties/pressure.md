---
name: pressure
category: Fluid Properties
summary: Fluid property: pressure from a real-fluid (CoolProp) backend.
related: []
examples: []
tags: [pressure, property, fluid, coolprop]
references: []
---

# pressure

Returns the **pressure** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
pressure(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
