//! Implicit DAE `F(t, y, y') = 0` and `LINEARIZE`.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/dae/`
//! (7 files, 1,026 LOC) plus `ast/LinearizeSystem.java`.
//!
//! The Java reaches SUNDIALS IDA through JNA. A wasm build cannot, which is
//! also why it sidesteps the SUNDIALS v6/v7 MPI ABI trap the parent documents.
//! Evaluate a pure-Rust path here; the numerical contract is what must match,
//! not the library.
pub mod assembly;
pub mod jacobian;
pub mod linearize;
pub mod solver;
