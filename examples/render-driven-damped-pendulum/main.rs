#[path = "../common/mod.rs"]
mod common;

/// Run the default example for rendering the Driven-Damped-Pendulum basin of attraction.
/// ```sh
/// cargo run --example render-driven-damped-pendulum
/// ```
///
/// Note that this example will take a bit of time to run -- this fractal is computationally
/// intensive, especially with the anti-aliasing enabled. This takes 1-2 minutes on my machine.
fn main() {
    common::render_example_from_string("render-driven-damped-pendulum")
}
