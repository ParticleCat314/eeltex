//! The data model: AST, scene, fonts, styles and errors.

pub(crate) mod alphabet;
pub(crate) mod ast;
pub mod attrs;
#[cfg(feature = "embed-math-font")]
pub(crate) mod embed;
pub(crate) mod error;
pub(crate) mod font;
pub(crate) mod scene;
pub(crate) mod style;
pub(crate) mod tex;
