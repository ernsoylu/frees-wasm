---
name: compressibilityfactor
category: Fluid Properties
summary: Fluid property: compressibilityfactor from a real-fluid (CoolProp) backend.
related: []
examples: [thermo-compliance]
tags: [compressibilityfactor, property, fluid, coolprop]
references: []
---

# compressibilityfactor

Returns the **compressibilityfactor** of a real fluid from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
compressibilityfactor(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
