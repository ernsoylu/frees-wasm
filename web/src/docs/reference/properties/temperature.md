---
name: temperature
category: Fluid Properties
summary: Fluid property: temperature from a real-fluid (CoolProp) backend.
related: []
examples: [pressure-cooker]
tags: [temperature, property, fluid, coolprop]
references: []
---

# temperature

Returns the **temperature** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
temperature(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
