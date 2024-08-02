use palette::{rgb::Rgb, FromColor, Hsl, IntoColor, LinSrgb, Mix, Srgb};
use serde::{Deserialize, Serialize};

/**

// Nice color map and framing here:
https://commons.wikimedia.org/wiki/File:Mandel_zoom_00_mandelbrot_set.jpg

The colors in the Mandelbrot set color map specification are given in hexadecimal format.
To convert these to RGB, we'll decode each hex color value into its respective

 red, green, and blue components. The format for each color is `index=color`,
  where the color is a decimal representation of the hexadecimal color value.
   Here are the color conversions:
   1. `6555392` in hexadecimal is `0x0063C0`.
   2. `13331232` in hexadecimal is `0xCBBA40`.
   3. `16777197` in hexadecimal is `0xFFFFFD`.
   4. `43775` in hexadecimal is `0x00AABF`.
   5. `3146289` in hexadecimal is `0x3001E1`.

   Now, let's convert these hexadecimal values to RGB:

   1. `0x0063C0`: - Red: `00` (0) - Green: `63` (99) - Blue: `C0` (192) RGB: (0, 99, 192)
   2. `0xCBBA40`: - Red: `CB` (203) - Green: `BA` (186) - Blue: `40` (64) RGB: (203, 186, 64)
   3. `0xFFFFFD`: - Red: `FF` (255) - Green: `FF` (255) - Blue: `FD` (253) RGB: (255, 255, 253)
   4. `0x00AABF`: - Red: `00` (0) - Green: `AA` (170) - Blue: `BF` (191) RGB: (0, 170, 191)
   5. `0x3001E1`: - Red: `30` (48) - Green: `01` (1) - Blue: `E1` (225) RGB: (48, 1, 225)

   To summarize:
   1. `6555392` -> RGB (0, 99, 192)
   2. `13331232` -> RGB (203, 186, 64)
   3. `16777197` -> RGB (255, 255, 253)
   4. `43775` -> RGB (0, 170, 191)
   5. `3146289` -> RGB (48, 1, 225)

 *
 */

/**
 * TODO:  docs
 */
#[derive(Serialize, Deserialize, Debug)]
pub struct ColorMapKeyFrame {
    pub query: f32,       // specify location of this color within the map; on [0,1]
    pub rgb_raw: [u8; 3], // [R, G, B]
}

/**
 * The keyframes are all in "raw RGB" data, but we can convert to alternate
 * representations behind the scenes to achieve different interpolation styles.
 */
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum InterpolationMode {
    Direct,
    Srgb,
    Hsl,
    Hsv,
    Lch,
    Lab,
}

/**
 * TODO:  docs
 */
pub struct PiecewiseLinearColorMap {
    keyframes: Vec<ColorMapKeyFrame>,
}

impl PiecewiseLinearColorMap {
    // TODO:  docs
    // TODO:  better error messages
    pub fn new(keyframes: Vec<ColorMapKeyFrame>) -> PiecewiseLinearColorMap {
        if keyframes.is_empty() {
            println!("ERROR:  keyframes are empty!");
            panic!();
        }

        if keyframes.first().unwrap().query != 0.0 {
            println!("ERROR:  initial keyframe query point must be 0.0!");
            panic!();
        }
        if keyframes.last().unwrap().query != 1.0 {
            println!("ERROR:  final keyframe query point must be 1.0!");
            panic!();
        }

        for i in 0..(keyframes.len() - 1) {
            if keyframes[i].query >= keyframes[i + 1].query {
                println!("ERROR:  keyframes should be monotonic, but are not!");
                panic!();
            }
        }
        PiecewiseLinearColorMap { keyframes }
    }

    pub fn compute(&self, query: f32, interpolation_mode: InterpolationMode) -> [u8; 3] {
        if query <= 0.0f32 {
            self.keyframes.first().unwrap().rgb_raw
        } else if query >= 1.0f32 {
            self.keyframes.last().unwrap().rgb_raw
        } else {
            let (i, j) = self.linear_index_search(query);
            let alpha = (query - self.keyframes[i].query)
                / (self.keyframes[j].query - self.keyframes[i].query);
            PiecewiseLinearColorMap::interpolate(
                &self.keyframes[i].rgb_raw,
                &self.keyframes[j].rgb_raw,
                alpha,
                interpolation_mode,
            )
        }
    }
    fn linear_index_search(&self, query: f32) -> (usize, usize) {
        let mut idx_low = self.keyframes.len() / 2;

        // hard limit on upper iteration, to catch bugs
        for _ in 0..self.keyframes.len() {
            if query < self.keyframes[idx_low].query {
                idx_low -= 1;
                continue;
            }
            if query >= self.keyframes[idx_low + 1].query {
                idx_low += 1;
                continue;
            }
            // [low <= query < upp]  --> success!
            return (idx_low, idx_low + 1);
        }

        println!("ERROR:  Linear keyframe search failed!");
        panic!();
    }

    fn interpolate(
        low: &[u8; 3],
        upp: &[u8; 3],
        alpha: f32,
        interpolation_mode: InterpolationMode,
    ) -> [u8; 3] {
        match interpolation_mode {
            InterpolationMode::Direct => {
                PiecewiseLinearColorMap::direct_interpolate(low, upp, alpha)
            }

            InterpolationMode::Srgb => PiecewiseLinearColorMap::lin_srgb_interpolate(low, upp, alpha),
            InterpolationMode::Hsl => PiecewiseLinearColorMap::hsl_interpolate(low, upp, alpha),
            InterpolationMode::Hsv => PiecewiseLinearColorMap::hsv_interpolate(low, upp, alpha),
            InterpolationMode::Lch => PiecewiseLinearColorMap::lch_interpolate(low, upp, alpha),
            InterpolationMode::Lab => PiecewiseLinearColorMap::lab_interpolate(low, upp, alpha),
        }
    }

    fn direct_interpolate(low_raw: &[u8; 3], upp_raw: &[u8; 3], alpha: f32) -> [u8; 3] {
        // Convert low and upp from [u8; 3] to Srgb using from_format
        let low_srgb: Rgb = Srgb::from_format((*low_raw).into());
        let upp_srgb: Rgb = Srgb::from_format((*upp_raw).into());

        // Interpolate between the two colors in the sRGB color space
        let interp_srgb = low_srgb.mix(upp_srgb, alpha);

        // Convert back to [u8; 3] using into_format
        interp_srgb.into_format().into()
    }

    fn lin_srgb_interpolate(low_raw: &[u8; 3], upp_raw: &[u8; 3], alpha: f32) -> [u8; 3] {
        let low_srgb: Rgb = Srgb::from_format((*low_raw).into());
        let upp_srgb: Rgb = Srgb::from_format((*upp_raw).into());
        let low_lin_srgb = low_srgb.into_linear();
        let upp_lin_srgb = upp_srgb.into_linear();
        let interp_srgb_lin = low_lin_srgb.mix(upp_lin_srgb, alpha);
        let srgb: Srgb = interp_srgb_lin.into_color();
        srgb.into_format().into()
    }

    fn hsl_interpolate(low_raw: &[u8; 3], upp_raw: &[u8; 3], alpha: f32) -> [u8; 3] {
        let low_srgb: Rgb = Srgb::from_format((*low_raw).into());
        let upp_srgb: Rgb = Srgb::from_format((*upp_raw).into());
        let low_lin_srgb = low_srgb.into_linear();
        let upp_lin_srgb = upp_srgb.into_linear();
        let low_hsl = Hsl::from_color(low_lin_srgb);
        let upp_hsl = Hsl::from_color(upp_lin_srgb);
        let interp_hsl = low_hsl.mix(upp_hsl, alpha);
        let interp_lin_srgb = LinSrgb::from_color(interp_hsl);
        let srgb: Srgb = interp_lin_srgb.into_color();
        srgb.into_format().into()
    }


    fn hsv_interpolate(low_raw: &[u8; 3], upp_raw: &[u8; 3], alpha: f32) -> [u8; 3] {
        let low_srgb: Rgb = Srgb::from_format((*low_raw).into());
        let upp_srgb: Rgb = Srgb::from_format((*upp_raw).into());
        let low_lin_srgb = low_srgb.into_linear();
        let upp_lin_srgb = upp_srgb.into_linear();
        let low_hsv = Hsl::from_color(low_lin_srgb);
        let upp_hsv = Hsl::from_color(upp_lin_srgb);
        let interp_hsv = low_hsv.mix(upp_hsv, alpha);
        let interp_lin_srgb = LinSrgb::from_color(interp_hsv);
        let srgb: Srgb = interp_lin_srgb.into_color();
        srgb.into_format().into()
    }


    fn lch_interpolate(low_raw: &[u8; 3], upp_raw: &[u8; 3], alpha: f32) -> [u8; 3] {
        let low_srgb: Rgb = Srgb::from_format((*low_raw).into());
        let upp_srgb: Rgb = Srgb::from_format((*upp_raw).into());
        let low_lin_srgb = low_srgb.into_linear();
        let upp_lin_srgb = upp_srgb.into_linear();
        let low_lch = Hsl::from_color(low_lin_srgb);
        let upp_lch = Hsl::from_color(upp_lin_srgb);
        let interp_lch = low_lch.mix(upp_lch, alpha);
        let interp_lin_srgb = LinSrgb::from_color(interp_lch);
        let srgb: Srgb = interp_lin_srgb.into_color();
        srgb.into_format().into()
    }

    fn lab_interpolate(low_raw: &[u8; 3], upp_raw: &[u8; 3], alpha: f32) -> [u8; 3] {
        let low_srgb: Rgb = Srgb::from_format((*low_raw).into());
        let upp_srgb: Rgb = Srgb::from_format((*upp_raw).into());
        let low_lin_srgb = low_srgb.into_linear();
        let upp_lin_srgb = upp_srgb.into_linear();
        let low_lab = Hsl::from_color(low_lin_srgb);
        let upp_lab = Hsl::from_color(upp_lin_srgb);
        let interp_lab = low_lab.mix(upp_lab, alpha);
        let interp_lin_srgb = LinSrgb::from_color(interp_lab);
        let srgb: Srgb = interp_lin_srgb.into_color();
        srgb.into_format().into()
    }

}

/**
 * Evaluates a cubic solving the boundary conditions:
 * f(0) = f'(0) = f'(1) == 0
 * f(1) = 1
 */
pub fn cubic_interpolation_map(x: f32) -> f32 {
    // Efficient calculation using Horner's method for polynomial evaluation
  x * (x * (-2.0 * x + 3.0))
}


#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use crate::core::color_map::cubic_interpolation_map;

    #[test]
    fn test_cubic_interpolation_map() {
        let h = 1e-3;
        let tol_y = 1e-6;
        let tol_dy = 5e-3;
        let y0 = cubic_interpolation_map(0.0);
        let dy0 = (cubic_interpolation_map(0.0 + h) - y0) / h;
        let y1 = cubic_interpolation_map(1.0);
        let dy1 = (y1 -cubic_interpolation_map(1.0 + h)) / h;
        assert_relative_eq!(y0, 0.0, epsilon  = tol_y);
        assert_relative_eq!(dy0, 0.0, epsilon = tol_dy);
        assert_relative_eq!(y1, 1.0, epsilon = tol_y);
        assert_relative_eq!(dy1, 0.0, epsilon = tol_dy);
    }
}