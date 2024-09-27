use std::process::Command;

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};

fn compute_file_hash(file_path: &str) -> Result<String, io::Error> {
    let mut file = File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer)?;

    hasher.update(&buffer);

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

fn check_file_hash(file_path: &str, expected_hash: &str) ->bool {
    match compute_file_hash(file_path) {
        Ok(computed_hash) => {
            if computed_hash == expected_hash {
                println!("Hash matches the expected value.");
                return true;
            } else {
                println!(
                    "Hash mismatch! Expected: {}, but got: {}",
                    expected_hash, computed_hash
                );
                return false;
            }
        }
        Err(e) => {
            eprintln!("Error reading the file: {:?}", e);
            return false;
        }
    }
}

fn run_command(command: &str, args: &[&str]) {
    let status = Command::new(command)
        .args(args)
        .status()
        .expect("failed to execute process");
    assert!(status.success(), "Command {:?} failed", command);
}
#[cfg(test)]
mod tests {
    use crate::{check_file_hash, run_command};

#[test]
fn test_mandelbrot_render() {
    run_command("cargo", &["run", "--release", "--", "render", "./tests/param_files/mandelbrot_default_tiny.json"]);
    let file_path = "out/render/mandelbrot/mandelbrot_default_tiny.png";
    let expected_hash = "559ef8eadaeab60dc8136933acd8b9eb7c589e69263ec995a5e526ad79b9ec14";

    assert!(check_file_hash(file_path, expected_hash));
}
}