use num_traits::Float;
use std::ops::{Add, Mul, Sub};

/// Trait for interpolation between two values
pub trait Interpolator<T, V>
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
{
    fn interpolate(&self, alpha: T, a: &V, b: &V) -> V;
}

/// Step interpolator switches from a to b at a threshold
#[derive(Default)]
pub struct StepInterpolator<T: Float + Copy> {
    pub threshold: T,
}

impl<T, V> Interpolator<T, V> for StepInterpolator<T>
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
{
    fn interpolate(&self, alpha: T, a: &V, b: &V) -> V {
        if alpha > self.threshold {
            *b
        } else {
            *a
        }
    }
}

/// Linear interpolation: a * (1 - alpha) + b * alpha
#[derive(Default)]
pub struct LinearInterpolator;

impl<T, V> Interpolator<T, V> for LinearInterpolator
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
{
    fn interpolate(&self, alpha: T, a: &V, b: &V) -> V {
        *a + (*b - *a) * alpha
    }
}
