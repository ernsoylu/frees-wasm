---
name: entropy
category: Fluid Properties
summary: Fluid property: entropy from the real-fluid property backend.
related: []
examples: [rankine-cycle, rankine-cycle, refrigeration-vcr]
tags: [entropy, property, fluid, coolprop]
references: []
---

# entropy

Returns the **entropy** of a real fluid from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
entropy(Fluid, P=, T=)
```

## Description

Supply the fluid name and any two independent state properties (T, P, h, s, x, …). Property names are case-insensitive.
