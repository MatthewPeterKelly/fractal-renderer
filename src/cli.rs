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
    #[clap(long, short)]
    pub rescale: f64,
}

impl Default for ParameterFilePath {
    fn default() -> Self {
        ParameterFilePath {
            params_path: String::default(),
            date_time_out: false,
            rescale: 1.0,
        }
    }
}
