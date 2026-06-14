use std::process::Command;

use image::io::Reader as ImageReader;
use sha2::{Digest, Sha256};

// We can't actually check the hash of the file, because the file has will be slightly
// different on each platform. Instead, we import the image contents as a flat buffer of
// pixels, and then hash that, which should be stable across platforms.
fn compute_image_file_hash(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let img = ImageReader::open(path)?.decode()?;
    let rgba_image = img.to_rgba8();
    let pixel_data = rgba_image.as_raw();
    let mut hasher = Sha256::new();
    hasher.update(pixel_data);
    let hash_result = hasher.finalize();
    Ok(format!("{:x}", hash_result))
}

fn check_image_file_hash(file_path: &str, expected_hash: &str) -> Result<(), String> {
    match compute_image_file_hash(file_path) {
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

    assert!(
        status.success(),
        "Command {:?} with args {:?} failed",
        command,
        args
    );
}
fn run_cargo_release_with_two_args(one: &str, two: &str) {
    run_command("cargo", &["run", "--release", "--", one, two]);
}

fn render_image_and_verify_file_hash(test_param_file_name_base: &str, expected_hash: &str) -> bool {
    run_cargo_release_with_two_args(
        "render",
        &format!("./tests/param_files/{}.json", test_param_file_name_base),
    );
    let image_file_path = format!("out/render/{}.png", test_param_file_name_base);
    match check_image_file_hash(&image_file_path, expected_hash) {
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
                "e731341fb865701eb19ac82123bd66d0c27695b2c6bdfed91b6030e155751283",
            ),
            (
                "mandelbrot/anti_aliasing_regression_test",
                "5c4d5ed86f4cbe74f4ef46bb63613746e0597ff88e33f5745b37571b7a1676ba",
            ),
            (
                "mandelbrot/downsample_interpolation_regression_test",
                "721e538c36b9cc78d62503f263cf51aaf89b1dc7b37ac0c5ae5085b97a1f65d5",
            ),
            (
                "julia/default_regression_test",
                "69b3b390da75b5bd8f6eeca7afac86cf41864582e2b4514c8f003dd29aef9d11",
            ),
            (
                "barnsley_fern/default_regression_test",
                "a4605eabb0ecaec01d3decc4191430143b36e36820a1ec5a186c836ed7364dd4",
            ),
            (
                "driven_damped_pendulum/default_regression_test",
                "1ea33ab96e50e47ce09716977681e651df5ff784539bec61370634b0fdb8c7f0",
            ),
            (
                "sierpinski/default_regression_test",
                "d7776c07094689b9c994f69012eeacccebd0167ab6fcec30e67f73f8ca9cd4c5",
            ),
            (
                "newtons_method/roots_of_unity_4_regression_test",
                "ea9d9e208eb53f5d10a77fb90b0dd3f60c5934514f5063841bcc681ed9c5f51f",
            ),
            (
                "newtons_method/cosh_minus_one_regression_test",
                "90ed7f68c987263bf3bc3535f72bf8c96062dbce95f87c467245bf4b21aca1df",
            ),
        ];

        let mut ok = true;
        for (name, hash) in test_cases {
            if !render_image_and_verify_file_hash(name, hash) {
                ok = false;
            }
        }

        assert!(ok);
    }
}
