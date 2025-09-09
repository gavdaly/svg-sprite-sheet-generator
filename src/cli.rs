use clap::{Parser, Subcommand};

#[derive(Debug, Subcommand)]
pub enum Commands {
    Watch,
    Build,
}

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value = "sprite.svg")]
    pub file: String,
    #[arg(short, long, default_value = "svgs")]
    pub directory: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

pub fn parse() -> Args {
    Args::parse()
}
