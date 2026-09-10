mod app_initializer;
mod cli;
mod config;
mod constants;
mod error;
mod ime;
mod input_detection;
mod ipc;
mod language;
mod layout;
mod native;
mod service;
mod types;
mod ui;
mod visibility;

use clap::Parser;
use cli::ProgramArgs;
use error::ErrorHandler;

fn main() {
    if let Some(result) = ime::run_internal() {
        if let Err(error) = result {
            eprintln!("Chinese dictionary preparation failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    tracing_subscriber::fmt::init();

    let args = ProgramArgs::parse();

    if let Err(e) = app_initializer::AppInitializer::initialize_and_run(args) {
        ErrorHandler::fatal(e);
    }
}
