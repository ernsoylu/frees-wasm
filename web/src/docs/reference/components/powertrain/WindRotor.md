---
name: WindRotor
category: Component (powertrain)
summary: Acausal powertrain-domain component WindRotor with ports shaft, wind, pitch.
related: []
examples: []
tags: [windrotor, component, powertrain, acausal]
references: []
generated: true
---

# WindRotor

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
WindRotor inst(rho, R, cp$, epsv, epsw)
```

## Ports

`shaft`, `wind`, `pitch`

## Parameters

| Parameter | Type |
| --- | --- |
| `rho` | Number |
| `R` | Number |
| `cp$` | String |
| `epsv` | Number |
| `epsw` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
lam       = shaft.w * R / (wind.sig + epsv)
Cpw       = cp$(lam, pitch.sig)
Pw        = 0.5 * rho * pi# * R^2 * wind.sig^3 * Cpw
shaft.tau = -Pw / (shaft.w + epsw)
```
