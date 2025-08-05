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

/// Keyframes, used to construct (and define) a piecewise interpolator
/// Generic keyframe: maps an input (query) to an output value.
#[derive(Clone, Copy)]
pub struct InterpolationKeyframe<T, V> {
    pub input: T,
    pub output: V,
}

/// Generic container for performing interpolation between keyframes
pub struct KeyframeInterpolator<T, V, F>
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
    F: Interpolator<T, V>,
{
    queries: Vec<T>,
    values: Vec<V>,
    interpolator: F,
}

impl<T, V, F> KeyframeInterpolator<T, V, F>
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
    F: Interpolator<T, V>,
{
    pub fn new(keyframes: Vec<InterpolationKeyframe<T, V>>, interpolator: F) -> Self {
        assert!(!keyframes.is_empty(), "keyframes must not be empty");

        for pair in keyframes.windows(2) {
            assert!(
                pair[0].input < pair[1].input,
                "keyframes must be strictly increasing"
            );
        }

        let queries = keyframes.iter().map(|k| k.input).collect();
        let values = keyframes.iter().map(|k| k.output).collect();

        Self {
            queries,
            values,
            interpolator,
        }
    }

    /// Evaluates the value of the trajectory by interpolating between keyframes.
    /// The query will be clamped to the valid domain of the keyframes (no extrapolation).
    pub fn evaluate(&self, query: T) -> V {
        if query <= *self.queries.first().unwrap() {
            self.values.first().copied().unwrap()
        } else if query >= *self.queries.last().unwrap() {
            self.values.last().copied().unwrap()
        } else {
            let idx_upp = self.queries.partition_point(|q| query >= *q);
            let idx_low = idx_upp - 1;
            let val_low = self.queries[idx_low];
            let alpha = (query - val_low) / (self.queries[idx_upp] - val_low);
            self.interpolator
                .interpolate(alpha, &self.values[idx_low], &self.values[idx_upp])
        }
    }
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
/// Extrapolate if alpha is not in [0,1]
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

//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::Vector3;

    #[test]
    fn test_linear_interpolator_scalar() {
        let interp = LinearInterpolator;
        let a: f32 = 10.0;
        let b: f32 = 20.0;
        // Keyframes
        assert_relative_eq!(interp.interpolate(0.0, &a, &b), 10.0, epsilon = 1e-6);
        assert_relative_eq!(interp.interpolate(1.0, &a, &b), 20.0, epsilon = 1e-6);
        // interpolation and extrapolation
        assert_relative_eq!(interp.interpolate(0.5, &a, &b), 15.0, epsilon = 1e-6);
        assert_relative_eq!(interp.interpolate(1.5, &a, &b), 25.0, epsilon = 1e-6);
    }

    #[test]
    fn test_step_interpolator_scalar() {
        let interp = StepInterpolator { threshold: 0.5 };
        let a: f64 = 1.0;
        let b: f64 = 5.0;
        assert_eq!(interp.interpolate(-0.5, &a, &b), 1.0);
        assert_eq!(interp.interpolate(0.0, &a, &b), 1.0);
        assert_eq!(interp.interpolate(0.49999, &a, &b), 1.0);
        assert_eq!(interp.interpolate(0.5, &a, &b), 1.0);
        assert_eq!(interp.interpolate(0.50001, &a, &b), 5.0);
        assert_eq!(interp.interpolate(1.0, &a, &b), 5.0);
        assert_eq!(interp.interpolate(1.5, &a, &b), 5.0);
    }

    #[test]
    fn test_linear_interpolator_vector() {
        let interp = LinearInterpolator;
        let a = Vector3::new(-5.0_f32, 2.0, 6.0);
        let b = Vector3::new(10.0_f32, 20.0, 30.0);
        assert_relative_eq!(
            interp.interpolate(0.0, &a, &b),
            a,
            epsilon = 1e-6
        );
        assert_relative_eq!(
            interp.interpolate(1.0, &a, &b),
            b,
            epsilon = 1e-6
        );
        assert_relative_eq!(
            interp.interpolate(0.3, &a, &b),
            0.3 * a + 0.7 * b,
            epsilon = 1e-6
        );
    }

    #[test]
    fn test_keyframe_interpolator_scalar_linear() {
        let keyframes: Vec<InterpolationKeyframe<f32, f32>> = vec![
            InterpolationKeyframe {
                input: -2.0,
                output: 1.0,
            },
            InterpolationKeyframe {
                input: 2.0,
                output: 11.0,
            },
            InterpolationKeyframe {
                input: 6.0,
                output: 12.0,
            },
        ];
        let interp = KeyframeInterpolator::new(keyframes, LinearInterpolator);
        assert_relative_eq!(interp.evaluate(-6.0), 1.0, epsilon = 1e-6); // extrapolate (clamped)
        assert_relative_eq!(interp.evaluate(-2.0), 1.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(1.0), 8.5, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(2.0), 11.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(3.0), 11.25, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(6.0), 12.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(8.0), 12.0, epsilon = 1e-6); // extrapolate (clamped)
    }

    #[test]
    fn test_keyframe_interpolator_vector_linear() {
        let low = Vector3::new(10.0, 3.0, 4.0);
        let upp = Vector3::new(100.0, 50.0, -30.0);
        let keyframes: Vec<InterpolationKeyframe<f32, Vector3<f32>>> = vec![
            InterpolationKeyframe {
                input: -3.0,
                output: low,
            },
            InterpolationKeyframe {
                input: 9.0,
                output: upp,
            },
        ];
        let interp = KeyframeInterpolator::new(keyframes, LinearInterpolator);

        assert_relative_eq!(interp.evaluate(-4.0), low, epsilon = 1e-6); // clamped extrapolate
        assert_relative_eq!(interp.evaluate(-3.0), low, epsilon = 1e-6); // keyframe
        assert_relative_eq!(
            interp.evaluate(3.0),
            0.5 * (low + upp),
            epsilon = 1e-6
        ); // interpolate
        assert_relative_eq!(
            interp.evaluate(6.0),
            0.25 * low + 0.75 * upp,
            epsilon = 1e-6
        ); // interpolate
        assert_relative_eq!(interp.evaluate(9.0), upp, epsilon = 1e-6); // keyframe
        assert_relative_eq!(interp.evaluate(100.0), low, epsilon = 1e-6); // clamped extrapolate
    }

    #[test]
    #[should_panic(expected = "keyframes must not be empty")]
    fn test_empty_keyframe_panics() {
        let _ = KeyframeInterpolator::<f32, f32, _>::new(vec![], LinearInterpolator);
    }

    #[test]
    #[should_panic(expected = "keyframes must be strictly increasing")]
    fn test_non_monotonic_keyframes_panics() {
        let keyframes = vec![
            InterpolationKeyframe {
                input: 0.0f32,
                output: 1.0,
            },
            InterpolationKeyframe {
                input: 0.5,
                output: 2.0,
            },
            InterpolationKeyframe {
                input: 0.5,
                output: 3.0,
            },
            InterpolationKeyframe {
                input: 1.0,
                output: 4.0,
            },
        ];
        let _ = KeyframeInterpolator::new(keyframes, LinearInterpolator);
    }
}
