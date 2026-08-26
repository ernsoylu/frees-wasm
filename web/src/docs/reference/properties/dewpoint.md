---
name: dewpoint
category: Fluid Properties
summary: Humid-air property: dewpoint from the real-fluid property backend.
related: []
examples: []
tags: [dewpoint, property, humid-air, coolprop]
references: []
---

# dewpoint

Returns the **dewpoint** of a humid-air (AirH2O) from any valid pair of independent state properties (rustprop, a pure-Rust port of CoolProp 8.0.0).

> Real-fluid/material/symbolic operation — see the inputs and references below.

## Syntax

```
dewpoint(AirH2O, T=, P=, R=)
```

## Description

A humid-air property; supply the dry-bulb T, total pressure P, and one humidity coordinate (R, W, B, or D). Property names are case-insensitive.
