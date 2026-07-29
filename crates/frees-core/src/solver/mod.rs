//! Steady solve: Tarjan blocking then Newton per block.

pub mod blocker;
pub mod newton;

pub use blocker::{Block, BlockingReport};
pub use newton::{newton_solve, NewtonReport, SolverSettings};
