use anyhow::Error;

pub struct ErrorHandler;

impl ErrorHandler {
    pub fn fatal(error: Error) -> ! {
        tracing::error!("Fatal error: {:#}", error);
        std::process::exit(1);
    }
}
