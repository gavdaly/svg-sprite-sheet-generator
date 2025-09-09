mod cli;
mod error;
mod svg;

use crate::error::AppError;
use cli::{Args, Commands};
use std::error::Error as _;

fn main() {
    let Args {
        file,
        directory,
        command,
    } = cli::parse();

    let result: Result<(), AppError> = match command {
        None => svg::process(&directory, &file),
        Some(Commands::Build) => svg::process(&directory, &file),
        Some(Commands::Watch) => Err(AppError::Unimplemented("watch subcommand")),
    };

    match result {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("\x1b[1;31mError:\x1b[0m {e}");
            if let Some(source) = e.source() {
                eprintln!("Caused by: {source}");
            }
            std::process::exit(1)
        }
    }
}
