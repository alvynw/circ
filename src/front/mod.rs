//! Input language front-ends

pub mod zokrates;
pub mod cerebro;

use super::ir::term::Computation;

/// A front-end
pub trait FrontEnd {
    /// Representation of an input program (possibly with argument assignments) for this language
    type Inputs;

    /// Compile the program (and possibly assignment) to constraints
    fn gen(i: Self::Inputs) -> Computation;
}
