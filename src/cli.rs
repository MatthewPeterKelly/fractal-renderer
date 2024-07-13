use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum RenderParams {
    Mandelbrot(crate::mandelbrot_core::MandelbrotParams),
    MandelbrotSearch(crate::mandelbrot_search::MandelbrotSearchParams),
    DrivenDampedPendulum(crate::ddp_utils::DrivenDampedPendulumParams),
    BarnsleyFern(crate::barnsley_fern::BarnsleyFernParams),
    Serpinsky(crate::serpinsky::SerpinskyParams),
}

#[derive(Debug, Parser)]
#[clap(author, version, about)]
pub struct FractalRendererArgs {
    #[command(subcommand)]
    pub command: Option<CommandsEnum>,
}

#[derive(Debug, Subcommand)]
pub enum CommandsEnum {
    MandelbrotRender(ParameterFilePath),
    DrivenDampedPendulumRender(ParameterFilePath),
    BarnsleyFernRender(ParameterFilePath),
    SerpinskyRender(ParameterFilePath),
    Render(ParameterFilePath),
}

#[derive(Debug, Args)]
pub struct ParameterFilePath {
    pub params_path: String,

    #[clap(long, short)]
    pub date_time_out: bool,

    // Note: so far, only the Mandelbrot render supports the following options.
    #[clap(long, short)]
    pub rescale: Option<f64>,

    #[clap(
        short,
        long,
        allow_negative_numbers = true,
        value_delimiter = ' ',
        num_args = 2
    )]
    pub translate: Option<Vec<f64>>,
}
