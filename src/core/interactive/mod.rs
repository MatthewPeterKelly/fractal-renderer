//! Unified interactive fractal explorer.
//!
//! Houses the `eframe` application that drives fractal exploration, together
//! with the live color-map editor panel shown alongside the preview.

pub mod app;
pub mod editor;

pub use app::explore;
