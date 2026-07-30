---
name: compressibility
category: Fluid Properties
summary: Fluid property: compressibility from a real-fluid (CoolProp) backend.
related: []
examples: [thermo-compliance]
tags: [compressibility, property, fluid, coolprop]
references: []
---

# compressibility

Returns the **compressibility** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
compressibility(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
