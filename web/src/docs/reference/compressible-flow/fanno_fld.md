---
name: fanno_fld
category: Compressible Flow
summary: Fanno friction parameter 4*f*Lmax/D
related: []
examples: []
tags: [fanno, fld, compressible, flow]
---

# fanno_fld

Fanno friction parameter 4*f*Lmax/D


## Syntax

```
fanno_fLD(M, k)
```

## Description

Fanno friction parameter 4*f*Lmax/D

## Mathematical Formulation

$$ \frac{4 f L^*}{D} = \frac{1-M^2}{kM^2} + \frac{k+1}{2k}\ln\frac{(k+1)M^2}{2 + (k-1)M^2} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
