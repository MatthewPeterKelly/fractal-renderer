use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[clap(author, version, about)]
pub struct FractalRendererArgs {
    #[command(subcommand)]
    pub command: Option<CommandsEnum>,
}

#[derive(Debug, Subcommand)]
pub enum CommandsEnum {
    MandelbrotRender(ParameterFilePath),
    MandelbrotSearch(ParameterFilePath),
    DrivenDampedPendulumRender(ParameterFilePath),
}

#[derive(Debug, Args)]
pub struct ParameterFilePath {
    pub params_path: String,

    #[clap(long, short)]
    pub date_time_out: bool,

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
