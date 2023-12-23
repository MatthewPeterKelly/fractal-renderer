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
    MandelbrotSearch(MandelbrotRandomSearch),
}

// Note:  `MandelbrotParamsOld` is for the CLI -- it is used to load `MandelbrotParams`
#[derive(Debug, Args)]
pub struct MandelbrotParamsOld {
    pub params_path: String,
}

// Let's make one for generating a sweep of mandelbrot params:
#[derive(Debug, Args)]
pub struct MandelbrotRandomSearch {
    pub params_path: String,
}
