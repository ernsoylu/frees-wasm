---
name: nu_plate
category: Heat Transfer
summary: Nu, chevron-angle dependent. SIDE: either single-phase stream in a brazed/gasketed PLATE HX. HX: plate heat exchanger (BPHE)
related: []
examples: []
tags: [nu, plate, heat, transfer]
---

# nu_plate

Nu, chevron-angle dependent. SIDE: either single-phase stream in a brazed/gasketed PLATE HX. HX: plate heat exchanger (BPHE)


## Syntax

```
nu_plate(Re, Pr, beta_deg)
```

## Description

Nu, chevron-angle dependent. SIDE: either single-phase stream in a brazed/gasketed PLATE HX. HX: plate heat exchanger (BPHE)

## Mathematical Formulation

$$ Nu = C(\beta)\,Re^{m}\,Pr^{1/3} \quad\text{(chevron plate, angle } \beta) $$

## Applicability

- **Where it applies:** A single-phase stream in a brazed/gasketed plate heat exchanger (BPHE).
- **Valid when:** Chevron-plate channels; depends on the chevron angle `β`.
- **How it's used:** Plate-side `h` for either stream of a plate HX.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re` | Number | Yes | Reynolds number. |
| `Pr` | Number | Yes | Prandtl number. |
| `beta_deg` | Number | Yes | Chevron / wave angle [deg]. |
