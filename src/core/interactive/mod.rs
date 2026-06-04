//! Unified interactive fractal explorer.
//!
//! Houses the `eframe` application that drives fractal exploration. Later
//! phases of the GUI-unification roadmap add a live color-map editor panel
//! alongside the preview; this module is the home for both.

pub mod app;
pub mod editor;

pub use app::explore;
