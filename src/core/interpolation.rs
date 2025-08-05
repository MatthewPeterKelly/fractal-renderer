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
        assert!(keyframes.first().unwrap().input == T::zero(), "first keyframe input must be 0.0");
        assert!(keyframes.last().unwrap().input == T::one(), "last keyframe input must be 1.0");

        for pair in keyframes.windows(2) {
            assert!(pair[0].input < pair[1].input, "keyframes must be strictly increasing");
        }

        let queries = keyframes.iter().map(|k| k.input).collect();
        let values = keyframes.iter().map(|k| k.output).collect();

        Self {
            queries,
            values,
            interpolator,
        }
    }

    pub fn evaluate(&self, query: T) -> V {
        if query <= T::zero() {
            self.values.first().copied().unwrap()
        } else if query >= T::one() {
            self.values.last().copied().unwrap()
        } else {
            let idx_upp = self.queries.partition_point(|q| query >= *q);
            let idx_low = idx_upp - 1;
            let val_low = self.queries[idx_low];
            let alpha = (query - val_low) / (self.queries[idx_upp] - val_low);
            self.interpolator.interpolate(alpha, &self.values[idx_low], &self.values[idx_upp])
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
