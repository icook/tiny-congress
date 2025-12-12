#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::use_self)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::float_cmp)]

pub mod ast;
pub mod attributes;
pub mod evaluator;

pub use ast::{Condition, Operator, Policy, Value};
pub use attributes::{fetch_attributes, AttributeContext};
pub use evaluator::authorize;
