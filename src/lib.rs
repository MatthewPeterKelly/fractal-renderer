#![deny(clippy::all)] // Clippy must be happy with all library code
#![forbid(unsafe_code)] // This library does not use any unsafe code blocks

pub mod cli;
pub mod core;
pub mod fractals;
