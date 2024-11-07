use serde::{Deserialize, Serialize};


use super::{
    barnsley_fern::BarnsleyFernParams, driven_damped_pendulum::DrivenDampedPendulumParams,
    julia::JuliaParams, mandelbrot::MandelbrotParams, serpinsky::SerpinskyParams,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum FractalParams {
    Mandelbrot(Box<MandelbrotParams>),
    Julia(Box<JuliaParams>),
    DrivenDampedPendulum(Box<DrivenDampedPendulumParams>),
    BarnsleyFern(Box<BarnsleyFernParams>),
    Serpinsky(Box<SerpinskyParams>),
}
