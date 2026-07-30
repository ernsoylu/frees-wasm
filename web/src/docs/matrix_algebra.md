[Topic: matrices-decl]
# Declaring Matrices & Vectors

frees uses array-language-like syntax for matrices and vectors. **You must declare the shape with a slice suffix** so the compiler can size the variable — the literal alone isn't enough.

## Notation
- **Vector:** `v[1:3] = [1, 2, 3]` — elements in brackets, comma-separated. The `[1:3]` declares a 3-element vector.
- **Matrix:** columns separated by commas, rows by semicolons:
```
A[1:2, 1:2] = [1, 2; 3, 4]
```
- **Slice suffixes:** `[start:end]` (1-based) tell the compiler the dimensions. Always include them on the left of an assignment when you first define an array; once sized, the bare name works downstream.

## Generation helpers
- **`zeros(m, n)`** — $m \times n$ zero matrix.
- **`ones(m, n)`** — $m \times n$ ones matrix.
- **`eye(n)` / `identity(n)`** — $n \times n$ identity.
- **`diag(v)`** — diagonal matrix from a vector, or the diagonal of a matrix.
- **`linspace(a, b, n)`** — $n$ values linearly spaced from $a$ to $b$.

## Examples
```
I[1:3, 1:3] = eye(3)                 { 3x3 identity }
grid[1:11] = linspace(0, 1, 11)      { 0, 0.1, …, 1.0 }
```

> **Tip:** for control-systems work, transfer-function coefficient arrays are just vectors — `num = [0, 0, 1]` and `den = [1, 3, 2]` represent $1/(s^2+3s+2)$. See *Control Systems & Symbolic CAS*.

[Related: matrices-ops, matrices-sys, arrays]

[Topic: matrices-ops]
# Matrix Operators

Standard algebraic operators work element-wise or as matrix operations depending on shape, sized automatically.

## Operators
- **`+` / `-`** — add/subtract same-shaped matrices (element-wise).
- **`*`** — matrix multiplication (sized automatically). A scalar times a matrix scales every element.
- **`'`** (postfix apostrophe) — transpose: `At = A'`.
- **`\`** (left division) — solves $A x = b$ directly: `x = A \ b`. Equivalent to `SolveLinear(A, b)`.

## Example: solve a 2×2 system
```
A[1:2, 1:2] = [1, 2; 3, 4]
b[1:2] = [5, 6]
x[1:2] = A \ b      { solves A * x = b }
```

[Related: matrices-sys, matrices-blas, matrices-decl]

[Topic: matrices-blas]
# OpenBLAS Algebra Functions

For high-performance vector/matrix operations, frees binds directly to BLAS (Basic Linear Algebra Subprograms). These are useful when you want explicit control over scaled updates.

## BLAS routines
- **`axpy(alpha, x, y)`** — $\alpha x + y$ (Level 1).
- **`scal(alpha, x)`** — $\alpha x$.
- **`asum(x)`** — $L_1$ norm (sum of absolute values).
- **`nrm2(x)`** — Euclidean $L_2$ norm.
- **`copy(x)`** — symbolic copy of a vector.
- **`gemv(alpha, A, x, beta, y)`** — $\alpha A x + \beta y$ (Level 2).
- **`ger(alpha, x, y, A)`** — outer-product update $\alpha x y^T + A$.
- **`gemm(alpha, A, B, beta, C)`** — $\alpha A B + \beta C$ (Level 3).

## Example
```
v1[1:3] = [1, 2, 3]
v2[1:3] = [4, 5, 6]
result[1:3] = axpy(2.5, v1[1:3], v2[1:3])   { 2.5*v1 + v2 }
```

[Related: matrices-sys, matrices-ops, ref-index]

[Topic: matrices-sys]
# Linear Systems & Decomposition

Dedicated routines for linear systems, decompositions, and structural analysis.

## Functions
- **`SolveLinear(A, b)`** — solve $A x = b$ (same as `A \ b`).
- **`Inverse(A)`** — $A^{-1}$.
- **`Determinant(A)`** — determinant of a square matrix.
- **`Dot(a, b)`** — vector dot product.
- **`Cross(a, b)`** — cross product of two 3-vectors.
- **`Norm(v)`** — Euclidean length of a vector.
- **`Eigenvalues(A)`** — eigenvalues of a square matrix.
- **`Eigen(A)`** — eigenvalues and eigenvectors.
- **`LUDecompose(A)`** — LU decomposition.
- **`EulerRotate(phi, theta, psi : R)`** — $3 \times 3$ rotation matrix from Euler angles (rad, ZYX), inside a `CALL`.

> **Control systems:** state-space models build directly on these matrix variables. LTI conversions (`tf2ss`, `ss2tf`), interconnection (`series`, `parallel`, `feedback`), analysis (`pole`, `zero`, `bode`, `nyquist`, `margin`, `step`, `impulse`, `lsim`), and controller design (`lqr`, `place`, `pidtune`) are documented under *Control Systems & Symbolic CAS*.

## Example: solve a 3×3 system
```
A[1:3, 1:3] = [2, 1, -1; -3, -1, 2; -2, 1, 2]
b[1:3] = [8, -11, -3]
x[1:3] = SolveLinear(A[1:3,1:3], b[1:3])
```

[Related: matrices-decl, symbolic-cas, ref-index]
