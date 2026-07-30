---
name: SigPID
category: Component (signal)
summary: Acausal signal-domain component SigPID with ports sp, pv, out.
related: []
examples: []
tags: [sigpid, component, signal, acausal]
references: []
generated: true
---

# SigPID

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigPID inst(Kp, Ki, Kd, tau, i0, d0, model$)
```

## Ports

`sp`, `pv`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `Kp` | Number |
| `Ki` | Number |
| `Kd` | Number |
| `tau` | Number |
| `i0` | Number |
| `d0` | Number |
| `model$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
e        = sp.sig - pv.sig
der(df)  = (e - df) / tau
init(df) = d0
dterm    = (e - df) / tau
init(ie) = i0
u_raw    = Kp * e + Ki * ie + Kd * dterm
```

## Model Variants

Selected via the `model$` parameter; each adds its own equations (and `REQUIRE`d parameters):

### `basic`

```
der(ie) = e
out.sig = u_raw
```

### `clamped` — requires `umin`, `umax`, `Taw`

```
out.sig = min(max(u_raw, umin), umax)
der(ie) = e + (out.sig - u_raw) / Taw
```
