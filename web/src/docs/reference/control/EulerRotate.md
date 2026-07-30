---
name: EulerRotate
category: Matrix
summary: 3×3 rotation matrix from Euler angles (φ, θ, ψ).
related: [Eigen, Transpose]
examples: []
tags: [matrix, rotation, euler angles, kinematics, attitude]
---

# EulerRotate

Returns the **3×3 rotation matrix** `R` corresponding to a sequence of Euler-angle
rotations `(φ, θ, ψ)`. Use it for rigid-body attitude, coordinate-frame transforms,
and vehicle/spacecraft kinematics.

## Syntax

```
CALL EulerRotate(phi, theta, psi : R)
R = EulerRotate(phi, theta, psi)
```

## Description

The angles are applied as elementary rotations about successive axes; the product
is an orthonormal rotation (`Rᵀ = R⁻¹`, `det R = 1`).

## Mathematical Formulation

The rotation is the product of three elementary rotations:

$$ R(\phi, \theta, \psi) = R_z(\psi)\,R_x(\theta)\,R_z(\phi), \qquad R^\top R = I,\ \det R = 1 $$

(the standard `z–x–z` convention).

> **Method:** multiply the three elementary axis rotations.

## Examples

```
{ R = EulerRotate(phi, theta, psi) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `phi` | Number | Yes | First rotation angle [rad]. |
| `theta` | Number | Yes | Second rotation angle [rad]. |
| `psi` | Number | Yes | Third rotation angle [rad]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `R` | Matrix | 3×3 orthonormal rotation matrix. |
