use nalgebra::Vector3;

pub trait Interpolator {
    fn interpolate(
        &self,
        query: f32,
        value_zero: &Vector3<f32>,
        value_one: &Vector3<f32>,
    ) -> Vector3<f32>;
}


#[derive(Default)]
pub struct StepInterpolator {
    pub threshold: f32,
}

impl Interpolator for StepInterpolator {
    fn interpolate(
        &self,
        query: f32,
        value_zero: &Vector3<f32>,
        value_one: &Vector3<f32>,
    ) -> Vector3<f32> {
        if query > self.threshold {
            *value_one
        } else {
            *value_zero
        }
    }
}

#[derive(Default)]
pub struct LinearInterpolator {}

impl Interpolator for LinearInterpolator {
    fn interpolate(
        &self,
        query: f32,
        value_zero: &Vector3<f32>,
        value_one: &Vector3<f32>,
    ) -> Vector3<f32> {
        value_zero + (value_one - value_zero) * query
    }
}


