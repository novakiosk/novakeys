use crate::service::CommandKind;
use clap::Parser;

/// Command-line argument structure for NOVA Keys application control.
#[derive(Parser, Debug, Clone)]
#[command(version)]
pub struct ProgramArgs {
    #[arg(short, long, action = clap::ArgAction::Append)]
    pub message: Vec<CommandKind>,

    /// Positional argument for the message (language code for set-language)
    pub value: Option<String>,
}
