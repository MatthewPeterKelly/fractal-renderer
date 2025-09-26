#[path = "../common/mod.rs"]
mod common;

/// Run the default example for rendering the Driven-Damped-Pendulum basin of attraction.
/// ```sh
/// cargo run --example render-driven-damped-pendulum-high-fidelity
/// ```
///
/// Note that this example will take a long time to run -- several minutes on my machine.
/// Edit: this takes a *long* time. Many tens of minutes.
fn main() {
    common::render_example_from_string("render-driven-damped-pendulum-high-fidelity")
}
