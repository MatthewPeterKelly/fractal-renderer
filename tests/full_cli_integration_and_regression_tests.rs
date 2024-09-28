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
    assert!(status.success(), "Command {:?} failed", command);
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
    use crate::{
        check_image_file_hash, render_image_and_verify_file_hash, run_cargo_release_with_two_args,
    };

    #[test]
    fn regression_test_cli_render_pipeline() {
        let test_cases = vec![
            (
                "mandelbrot/default_regression_test",
                "3b3929d109b890dcbc00eaa9ee502f806d6823636af3c3814b0bbccce740ed7a",
            ),
            (
                "barnsley_fern/default_regression_test",
                "a4605eabb0ecaec01d3decc4191430143b36e36820a1ec5a186c836ed7364dd4",
            ),
            // Disabled; Works locally, but not in CI. Details here:
            // https://github.com/MatthewPeterKelly/fractal-renderer/issues/90
            // (
            //     "driven_damped_pendulum/default_regression_test",
            //     "5f1bbcbe83afdc2ea36b34ce3774e5efc99bec3b426c80524bf0c4efb1097e7e",
            // ),
            (
                "serpinsky/default_regression_test",
                "d7776c07094689b9c994f69012eeacccebd0167ab6fcec30e67f73f8ca9cd4c5",
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

    #[test]
    fn regression_test_cli_color_swatch() {
        run_cargo_release_with_two_args(
            "color-swatch",
            "./tests/param_files/color_swatch/default_regression_test.json",
        );
        match check_image_file_hash(
            "out/color_swatch/default_regression_test.png",
            "a8d6ad55aa64624152a9fb9d867ce77aab1a05cf25956b8c6826cf6cd388cf51",
        ) {
            Ok(()) => {}
            Err(diagnostics) => {
                println!("Hash mismatch! Color swatch hash: {}", diagnostics);
                panic!()
            }
        }
    }
}
