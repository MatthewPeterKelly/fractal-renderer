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

fn check_file_hash(file_path: &str, expected_hash: &str) -> Result<(), String> {
    match compute_file_hash(file_path) {
        Ok(computed_hash) => {
            if computed_hash == expected_hash {
                Ok(())
            } else {
                Err(computed_hash)
            }
        }
        Err(e) => Err(format!("Unable to read file {}:{}", file_path, e)),
    }
}

fn run_command(command: &str, args: &[&str]) {
    let status = Command::new(command)
        .args(args)
        .status()
        .expect("failed to execute process");
    assert!(status.success(), "Command {:?} failed", command);
}

fn render_image_and_verify_file_hash(command_name: &str, test_param_file_name_base: &str, expected_hash: &str) -> bool {
    run_command(
        "cargo",
        &[
            "run",
            "--release",
            "--",
            command_name,
            &format!("./tests/param_files/{}.json", test_param_file_name_base),
        ],
    );
    let image_file_path = format!("out/{}/{}.png",command_name, test_param_file_name_base);
    match check_file_hash(&image_file_path, expected_hash) {
        Ok(()) => true,
        Err(diagnostics) => {
            println!(
                "Hash mismatch! Expected: {}, but got:\n\n(\"{}\",\"{}\"),\n",
                expected_hash, test_param_file_name_base, diagnostics
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::render_image_and_verify_file_hash;

    #[test]
    fn regression_test_cli_render_pipeline() {
        let test_cases = vec![
            (
                "mandelbrot/default_regression_test",
                "559ef8eadaeab60dc8136933acd8b9eb7c589e69263ec995a5e526ad79b9ec14",
            ),
            (
                "barnsley_fern/default_regression_test",
                "aca6adf73cd023de8cead344e3e9c685ab4b3f4f36c310e76c3c604eefe4b2fd",
            ),
            (
                "driven_damped_pendulum/default_regression_test",
                "cc86a883e363661b95f32346c986f98561e3e3e71cd0555a5afc9a9b18878633",
            ),
            (
                "serpinsky/default_regression_test",
                "e2a1fb8000f7ad9c73a64e190dc26e45db5f217a96a7227e99dbead4bc8191ca",
            ),
        ];

        let mut ok = true;
        for (name, hash) in test_cases {
            if !render_image_and_verify_file_hash("render", name, hash) {
                ok = false;
            }
        }

        if !render_image_and_verify_file_hash("color-swatch", "color-swatch/default_regression_test", "0") {
            ok = false;
        }

        assert!(ok);
    }
}
