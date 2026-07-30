---
name: BatteryCellMap
category: Component (electrical)
summary: Acausal electrical-domain component BatteryCellMap with ports p, n, heat.
related: []
examples: []
tags: [batterycellmap, component, electrical, acausal]
references: []
generated: true
---

# BatteryCellMap

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
BatteryCellMap inst(ocv$, dudt$, R0ref, Tref, Ea, Q0, C_th, SOC0, T0, k_age, model$)
```

## Ports

`p`, `n`, `heat`

## Parameters

| Parameter | Type |
| --- | --- |
| `ocv$` | String |
| `dudt$` | String |
| `R0ref` | Number |
| `Tref` | Number |
| `Ea` | Number |
| `Q0` | Number |
| `C_th` | Number |
| `SOC0` | Number |
| `T0` | Number |
| `k_age` | Number |
| `model$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
I         = -p.I
R0        = R0ref * exp((Ea / 8.314) * (1 / T - 1 / Tref))
Voc       = ocv$(SOC)
p.V - n.V = Voc - R0 * I
p.I + n.I = 0
der(SOC)  = -I / (3600 * Qcap)
init(SOC) = SOC0
Qgen      = R0 * I^2 - I * T * dudt$(SOC)
heat.T    = T
der(T)    = (Qgen + heat.Qdot) / C_th
init(T)   = T0
```

## Model Variants

Selected via the `model$` parameter; each adds its own equations (and `REQUIRE`d parameters):

### `static`

```
Qcap = Q0
```

### `aging` — requires `k_age`

```
der(Ah)  = abs(I) / 3600
init(Ah) = 0
Qcap     = Q0 * (1 - k_age * Ah)
```
