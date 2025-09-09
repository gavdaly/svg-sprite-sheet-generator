mod cli;
mod svg;

use cli::{Args, Commands};

fn main() {
    let Args {
        file,
        directory,
        command,
    } = cli::parse();

    let result = match command {
        None => svg::process(&directory, &file),
        Some(Commands::Build) => svg::process(&directory, &file),
        Some(Commands::Watch) => {
            println!("Comming Soon");
            Err(())
        }
    };

    match result {
        Ok(_) => std::process::exit(0),
        Err(_) => {
            eprintln!("\x1b[1;31mError!\x1b[0m");
            std::process::exit(-1)
        }
    }
}
