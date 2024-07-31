use serde::{Deserialize, Serialize};

use crate::core::{
    color_map::{ColorMapKeyFrame, PiecewiseLinearColorMap},
    file_io::{build_output_path_with_date_time, date_time_string, FilePrefix},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct ColorSwatchParams {
    pub resolution: (u32, u32),
    keyframes: Vec<ColorMapKeyFrame>,
}

fn generate_color_swatch(params_path: &str) -> _ {
    let params: ColorSwatchParams = serde_json::from_str(
        &std::fs::read_to_string(params_path).expect("Unable to read param file"),
    )
    .unwrap();

    let file_prefix = FilePrefix {
        directory_path: build_output_path_with_date_time(
            "color_swatch",
            "debug",
            &Some(date_time_string()),
        ),
        file_base: "colors".to_owned(), // HACK!!!!
    };

    std::fs::write(
        file_prefix.with_suffix(".json"),
        serde_json::to_string(&params).unwrap(),
    )
    .expect("Unable to write file");

    // Save the image to a file, deducing the type from the file name
    // Create a new ImgBuf to store the render in memory (and eventually write it to a file).
    let mut imgbuf = image::ImageBuffer::new(params.resolution.0, params.resolution.1);

    let colormap = PiecewiseLinearColorMap::new(params.keyframes);

    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let query = 0.5;
        *pixel = image::Rgb(colormap.compute(query));
    }

    let render_path = file_prefix.with_suffix(".png");
    imgbuf.save(&render_path).unwrap();
    println!("INFO:  Wrote image file to: {}", render_path.display());
}
