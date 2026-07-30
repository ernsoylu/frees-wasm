---
name: cp
category: Fluid Properties
summary: Fluid property: cp from a real-fluid (CoolProp) backend.
related: []
examples: []
tags: [cp, property, fluid, coolprop]
references: []
---

# cp

Returns the **cp** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
cp(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
