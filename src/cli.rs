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
}

#[derive(Debug, Args)]
pub struct ParameterFilePath {
    pub params_path: String,
    #[clap(long, short)]
    pub date_time_out: bool,
    #[clap(long, short, default_value = "1.0")]
    pub rescale: f64,
}
