use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[clap(author, version, about)]
pub struct FractalRendererArgs {
    #[command(subcommand)]
    pub command: Option<CommandsEnum>,
}

#[derive(Debug, Subcommand)]
pub enum CommandsEnum {
    Mandelbrot(ParameterFilePath),
    MandelbrotSearch(ParameterFilePath),
}

#[derive(Debug, Args)]
pub struct ParameterFilePath {
    pub params_path: String,
}
