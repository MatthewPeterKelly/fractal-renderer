use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[clap(author, version, about)]
pub struct FractalRendererArgs {
    #[command(subcommand)]
    pub command: Option<CommandsEnum>,
}

#[derive(Debug, Subcommand)]
pub enum CommandsEnum {
    Mandelbrot(MandelbrotParamsOld),
}

// Note:  `MandelbrotParamsOld` is for the CLI -- it is used to load `MandelbrotParams`
#[derive(Debug, Args)]
pub struct MandelbrotParamsOld {
    pub params_path: String,
}

