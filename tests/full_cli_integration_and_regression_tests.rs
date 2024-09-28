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

    Ok(format!("{:x}", hasher.finalize()))
}

fn check_file_hash(file_path: &str, expected_hash: &str) -> bool {
    match compute_file_hash(file_path) {
        Ok(computed_hash) => {
            if computed_hash == expected_hash {
                println!("Hash matches the expected value.");
                true
            } else {
                println!(
                    "Hash mismatch! Expected: {}, but got: {}",
                    expected_hash, computed_hash
                );
                false
            }
        }
        Err(e) => {
            eprintln!("Error reading the file: {:?}", e);
            false
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

fn render_image_and_verify_file_hash(test_param_file_name_base: &str, expected_hash: &str) -> bool {
    run_command(
        "cargo",
        &[
            "run",
            "--release",
            "--",
            "render",
            &format!("./tests/param_files/{}.json", test_param_file_name_base),
        ],
    );
    let image_file_path = format!("out/render/{}.png", test_param_file_name_base);
    check_file_hash(&image_file_path, expected_hash)
}

#[cfg(test)]
mod tests {
    use crate::render_image_and_verify_file_hash;

    #[test]
    fn test_mandelbrot_render() {
        let ok = render_image_and_verify_file_hash(
            "mandelbrot/default_regression_test",
            "559ef8eadaeab60dc8136933acd8b9eb7c589e69263ec995a5e526ad79b9ec14",
        );
        let ok = render_image_and_verify_file_hash(
            "barnsley_fern/default_regression_test",
            "0a0105e25e2f1ecc2376d850c0fa99c9251425798be5e9dc5d1a1e7db5cc6b90",
        );

        assert!(ok);
    }
}
