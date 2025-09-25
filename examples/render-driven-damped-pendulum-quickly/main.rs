#[path = "../common/mod.rs"]
mod common;

/// Run the default example for rendering the Driven-Damped-Pendulum basin of attraction.
/// ```sh
/// cargo run --example render-driven-damped-pendulum-quickly
/// ```
///
/// Note that this version of the DDP dropx the anti-aliasing level, and also the simulation
/// convergence criteria so that it runs quickly. This results in a grainier image... but it
/// runs in something like 10 seconds instead of minutes.
pub fn main() {
    common::render_example_from_string("render-driven-damped-pendulum-quickly")
}
