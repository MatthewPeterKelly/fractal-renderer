use std::process::Command;

fn run_command(command: &str, args: &[&str]) {
    let status = Command::new(command)
        .args(args)
        .status()
        .expect("failed to execute process");
    assert!(status.success(), "Command {:?} failed", command);
}
#[cfg(test)]
mod tests {
    use crate::run_command;

#[test]
fn test_mandelbrot_render() {
    run_command("cargo", &["run", "--release", "--", "render", "./tests/param_files/mandelbrot_default_tiny.json"]);
}
}