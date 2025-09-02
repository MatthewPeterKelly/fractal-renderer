use num_traits::Float;
use std::ops::{Add, Mul, Sub};

/// Trait for interpolation between two values
pub trait Interpolator<T, V>
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
{
    fn interpolate(&self, alpha: T, a: V, b: V) -> V;
}

/// Keyframes, used to construct (and define) a piecewise interpolator
/// Generic keyframe: maps an input (query) to an output value.
#[derive(Clone, Copy)]
pub struct InterpolationKeyframe<T, V> {
    pub query: T,
    pub value: V,
}

/// Generic container for performing interpolation between keyframes

#[derive(Clone, Debug)]
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
                pair[0].query < pair[1].query,
                "keyframes must be strictly increasing"
            );
        }

        let queries = keyframes.iter().map(|k| k.query).collect();
        let values = keyframes.iter().map(|k| k.value).collect();

        Self {
            queries,
            values,
            interpolator,
        }
    }

    #[cfg(test)]
    pub fn set_keyframe_query(&mut self, index: usize, query: T) {
        assert!(
            index < self.queries.len(),
            "Index out of bounds!  Cannot update keyframe query."
        );
        if index > 0 {
            assert!(
                self.queries[index - 1] < query,
                "The keyframes must remain strictly monotonic! Violation on lower edge."
            );
        }
        if index < (self.queries.len() - 1) {
            assert!(
                query < self.queries[index + 1],
                "The keyframes must remain strictly monotonic! Violation on upper edge."
            );
        }
        self.queries[index] = query;
    }

    #[cfg(test)]
    pub fn set_keyframe_value(&mut self, index: usize, value: V) {
        assert!(
            index < self.queries.len(),
            "Index out of bounds!  Cannot update keyframe value."
        );
        // No need to check monotonicity for output values, as they can be any value.
        self.values[index] = value;
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
                .interpolate(alpha, self.values[idx_low], self.values[idx_upp])
        }
    }
}

/// Step interpolator switches from the lower to upper value above the specified threshold
#[derive(Default, Clone, Copy, Debug)]
pub struct StepInterpolator<T: Float + Copy> {
    pub threshold: T,
}

impl<T, V> Interpolator<T, V> for StepInterpolator<T>
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
{
    fn interpolate(&self, alpha: T, low: V, upp: V) -> V {
        if alpha > self.threshold {
            upp
        } else {
            low
        }
    }
}

/// Linear interpolation: low * (1 - alpha) + upp * alpha
/// Extrapolate if alpha is not in [0,1]
#[derive(Default, Clone, Copy, Debug)]
pub struct LinearInterpolator;

impl<T, V> Interpolator<T, V> for LinearInterpolator
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
{
    /// Interpolates between the specified values
    /// - `alpha`: interpolation parameter, typically on [0,1]
    /// - `low`: lower bound on interpolation; returned if `alpha == 0.0`
    /// - `upp`: upper bound on interpolation; returned if `alpha == 1.0`
    ///
    /// Note:  this method will *extrapolate* if `alpha` is not in [0,1]
    fn interpolate(&self, alpha: T, low: V, upp: V) -> V {
        low + (upp - low) * alpha
    }
}

/// Clamped Linear interpolation: low * (1 - alpha.clamp(0,1)) + upp * alpha)
#[derive(Default, Clone, Copy, Debug)]
pub struct ClampedLinearInterpolator;

impl<T, V> Interpolator<T, V> for ClampedLinearInterpolator
where
    T: Float + Copy,
    V: Copy + Add<Output = V> + Sub<Output = V> + Mul<T, Output = V>,
{
    /// Interpolates between the specified values
    /// - `alpha`: interpolation parameter, typically on [0,1]
    /// - `low`: lower bound on interpolation; returned if `alpha == 0.0`
    /// - `upp`: upper bound on interpolation; returned if `alpha == 1.0`
    ///
    /// Note:  this method will clamp `alpha` to [0,1] --> [low, upp].
    fn interpolate(&self, alpha: T, low: V, upp: V) -> V {
        if alpha <= T::zero() {
            return low;
        }
        if alpha >= T::one() {
            return upp;
        }
        let interpolator = LinearInterpolator;
        interpolator.interpolate(alpha, low, upp)
    }
}

//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::Vector3;

    fn make_test_scalar_interpolator() -> KeyframeInterpolator<f32, f32, LinearInterpolator> {
        let keyframes: Vec<InterpolationKeyframe<f32, f32>> = vec![
            InterpolationKeyframe {
                query: -2.0,
                value: 1.0,
            },
            InterpolationKeyframe {
                query: 2.0,
                value: 11.0,
            },
            InterpolationKeyframe {
                query: 6.0,
                value: 12.0,
            },
        ];
        KeyframeInterpolator::new(keyframes, LinearInterpolator)
    }

    #[test]
    fn test_linear_interpolator_scalar() {
        let interp = LinearInterpolator;
        let low: f32 = 10.0;
        let upp: f32 = 20.0;
        // Keyframes
        assert_relative_eq!(interp.interpolate(0.0, low, upp), 10.0, epsilon = 1e-6);
        assert_relative_eq!(interp.interpolate(1.0, low, upp), 20.0, epsilon = 1e-6);
        // interpolation and extrapolation
        assert_relative_eq!(interp.interpolate(0.5, low, upp), 15.0, epsilon = 1e-6);
        assert_relative_eq!(interp.interpolate(1.5, low, upp), 25.0, epsilon = 1e-6);
    }

    #[test]
    fn test_clamped_linear_interpolator_scalar() {
        let interp = ClampedLinearInterpolator;
        let low: f32 = 10.0;
        let upp: f32 = 20.0;
        // Keyframes
        assert_relative_eq!(interp.interpolate(0.0, low, upp), 10.0, epsilon = 1e-6);
        assert_relative_eq!(interp.interpolate(1.0, low, upp), 20.0, epsilon = 1e-6);
        // interpolation
        assert_relative_eq!(interp.interpolate(0.5, low, upp), 15.0, epsilon = 1e-6);
        // extrapolation
        assert_relative_eq!(interp.interpolate(-0.6, low, upp), 10.0, epsilon = 1e-6);
        assert_relative_eq!(interp.interpolate(1.5, low, upp), 20.0, epsilon = 1e-6);
    }

    #[test]
    fn test_step_interpolator_scalar() {
        let interp = StepInterpolator { threshold: 0.5 };
        let low: f64 = 1.0;
        let upp: f64 = 5.0;
        assert_eq!(interp.interpolate(-0.5, low, upp), 1.0);
        assert_eq!(interp.interpolate(0.0, low, upp), 1.0);
        assert_eq!(interp.interpolate(0.49999, low, upp), 1.0);
        assert_eq!(interp.interpolate(0.5, low, upp), 1.0);
        assert_eq!(interp.interpolate(0.50001, low, upp), 5.0);
        assert_eq!(interp.interpolate(1.0, low, upp), 5.0);
        assert_eq!(interp.interpolate(1.5, low, upp), 5.0);
    }

    #[test]
    fn test_linear_interpolator_vector() {
        let interp = LinearInterpolator;
        let low = Vector3::new(-5.0_f32, 2.0, 6.0);
        let upp = Vector3::new(10.0_f32, 20.0, 30.0);
        assert_relative_eq!(interp.interpolate(0.0, low, upp), low, epsilon = 1e-6);
        assert_relative_eq!(interp.interpolate(1.0, low, upp), upp, epsilon = 1e-6);
        assert_relative_eq!(
            interp.interpolate(0.3, low, upp),
            0.7 * low + 0.3 * upp,
            epsilon = 1e-6
        );
    }

    #[test]
    fn test_keyframe_interpolator_scalar_linear() {
        let interp = make_test_scalar_interpolator();
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
                query: -3.0,
                value: low,
            },
            InterpolationKeyframe {
                query: 9.0,
                value: upp,
            },
        ];
        let interp = KeyframeInterpolator::new(keyframes, LinearInterpolator);

        assert_relative_eq!(interp.evaluate(-4.0), low, epsilon = 1e-6); // clamped extrapolate
        assert_relative_eq!(interp.evaluate(-3.0), low, epsilon = 1e-6); // keyframe
        assert_relative_eq!(interp.evaluate(3.0), 0.5 * (low + upp), epsilon = 1e-6); // interpolate
        assert_relative_eq!(
            interp.evaluate(6.0),
            0.25 * low + 0.75 * upp,
            epsilon = 1e-6
        ); // interpolate
        assert_relative_eq!(interp.evaluate(9.0), upp, epsilon = 1e-6); // keyframe
        assert_relative_eq!(interp.evaluate(100.0), upp, epsilon = 1e-6); // clamped extrapolate
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
                query: 0.0f32,
                value: 1.0,
            },
            InterpolationKeyframe {
                query: 0.5,
                value: 2.0,
            },
            InterpolationKeyframe {
                query: 0.5,
                value: 3.0,
            },
            InterpolationKeyframe {
                query: 1.0,
                value: 4.0,
            },
        ];
        let _ = KeyframeInterpolator::new(keyframes, LinearInterpolator);
    }

    #[test]
    fn test_keyframe_mutable_update() {
        let mut interp = make_test_scalar_interpolator();
        // Baseline!
        assert_relative_eq!(interp.evaluate(-6.0), 1.0, epsilon = 1e-6); // extrapolate (clamped)
        assert_relative_eq!(interp.evaluate(-2.0), 1.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(1.0), 8.5, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(2.0), 11.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(3.0), 11.25, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(6.0), 12.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(8.0), 12.0, epsilon = 1e-6); // extrapolate (clamped)

        // Now let's modify the query of the first keyframe:
        interp.set_keyframe_query(0, 0.0);
        assert_relative_eq!(interp.evaluate(-1.0), 1.0, epsilon = 1e-6); // extrapolate (clamped)
        assert_relative_eq!(interp.evaluate(-0.0), 1.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(1.0), 6.0, epsilon = 1e-6); // interpolate (updated!)
        assert_relative_eq!(interp.evaluate(2.0), 11.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(3.0), 11.25, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(6.0), 12.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(8.0), 12.0, epsilon = 1e-6); // extrapolate (clamped)

        // Now let's modify the value of the first keyframe:
        interp.set_keyframe_value(0, 20.0);
        assert_relative_eq!(interp.evaluate(-1.0), 20.0, epsilon = 1e-6); // extrapolate (clamped)
        assert_relative_eq!(interp.evaluate(-0.0), 20.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(1.0), 15.5, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(2.0), 11.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(3.0), 11.25, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(6.0), 12.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(8.0), 12.0, epsilon = 1e-6); // extrapolate (clamped)

        // Now let's modify the query of the last keyframe:
        interp.set_keyframe_query(2, 10.0);
        assert_relative_eq!(interp.evaluate(-1.0), 20.0, epsilon = 1e-6); // extrapolate (clamped)
        assert_relative_eq!(interp.evaluate(-0.0), 20.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(1.0), 15.5, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(2.0), 11.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(6.0), 11.5, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(10.0), 12.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(18.0), 12.0, epsilon = 1e-6); // extrapolate (clamped)

        // Now let's modify the value of the last keyframe:
        interp.set_keyframe_value(2, 0.0);
        assert_relative_eq!(interp.evaluate(-1.0), 20.0, epsilon = 1e-6); // extrapolate (clamped)
        assert_relative_eq!(interp.evaluate(-0.0), 20.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(1.0), 15.5, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(2.0), 11.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(6.0), 5.5, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(10.0), 0.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(18.0), 0.0, epsilon = 1e-6); // extrapolate (clamped)

        // And the middle keyframe
        interp.set_keyframe_query(1, 5.0);
        interp.set_keyframe_value(1, 200.0);
        assert_relative_eq!(interp.evaluate(-1.0), 20.0, epsilon = 1e-6); // extrapolate (clamped)
        assert_relative_eq!(interp.evaluate(-0.0), 20.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(3.0), 128.0, epsilon = 1e-6); // interpolate
        assert_relative_eq!(interp.evaluate(5.0), 200.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(7.0), 120.0, epsilon = 1e-6); // interpolateg
        assert_relative_eq!(interp.evaluate(10.0), 0.0, epsilon = 1e-6);
        assert_relative_eq!(interp.evaluate(18.0), 0.0, epsilon = 1e-6); // extrapolate (clamped)
    }

    #[test]
    #[should_panic(expected = "Index out of bounds!  Cannot update keyframe query.")]
    fn test_keyframe_update_panic_query_bounds() {
        let mut interp = make_test_scalar_interpolator();
        interp.set_keyframe_query(10, 5.0);
    }

    #[test]
    #[should_panic(expected = "Index out of bounds!  Cannot update keyframe value.")]
    fn test_keyframe_update_panic_value_bounds() {
        let mut interp = make_test_scalar_interpolator();
        interp.set_keyframe_value(10, 5.0);
    }

    #[test]
    #[should_panic(
        expected = "The keyframes must remain strictly monotonic! Violation on lower edge."
    )]
    fn test_keyframe_update_panic_query_low_monotonic() {
        let mut interp = make_test_scalar_interpolator();
        interp.set_keyframe_query(1, -100.0);
    }

    #[test]
    #[should_panic(
        expected = "The keyframes must remain strictly monotonic! Violation on upper edge."
    )]
    fn test_keyframe_update_panic_query_upp_monotonic() {
        let mut interp = make_test_scalar_interpolator();
        interp.set_keyframe_query(1, 100.0);
    }
}
