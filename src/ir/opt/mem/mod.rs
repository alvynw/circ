//! Memory optimizations

pub mod lin;
pub mod obliv;
mod visit;

use crate::ir::term::*;

/// Eliminates arrays, first oblivious ones, and then all arrays.
pub fn array_elim(t: &Term) -> Term {
    lin::linearize(&obliv::elim_obliv(t), usize::MAX)
}
