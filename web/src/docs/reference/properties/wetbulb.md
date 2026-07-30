---
name: wetbulb
category: Fluid Properties
summary: Humid-air property: wetbulb from a real-fluid (CoolProp) backend.
related: []
examples: []
tags: [wetbulb, property, humid-air, coolprop]
references: []
---

# wetbulb

Returns the **wetbulb** of a humid-air (AirH2O) from any valid pair of independent state properties (CoolProp backend).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
wetbulb(AirH2O, T=, P=, R=)
```

## Description

A humid-air property; supply the dry-bulb T, total pressure P, and one humidity coordinate (R, W, B, or D). Property names are case-insensitive.
