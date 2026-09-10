use std::sync::Arc;

use anyhow::{Context, Result};

use crate::{
    cli::ProgramArgs,
    config::AppConfig,
    input_detection::InputDetectionManager,
    ipc::Ipc,
    service::{client::MessageService, host::AppService},
    visibility::{KeyboardVisibilityManager, VisibilityManager},
};

pub struct AppInitializer;

impl AppInitializer {
    pub fn initialize_and_run(args: ProgramArgs) -> Result<()> {
        if !args.message.is_empty() {
            return MessageService::new(Ipc::client_only()?, args).run();
        }
        let ipc = Ipc::init()?;
        if !ipc.is_single_instance() {
            anyhow::bail!("NOVA Keys is already running");
        }
        Self::run_main_service(ipc)
    }

    fn run_main_service(ipc: Ipc) -> Result<()> {
        let app_config = AppConfig::with_config_loaded()?;
        let input_method =
            InputDetectionManager::new().context("Failed to initialize input method")?;
        let keyboard = input_method.keyboard();
        let visibility_manager =
            Arc::new(KeyboardVisibilityManager::new(&input_method)) as Arc<dyn VisibilityManager>;
        AppService::new(keyboard, ipc, app_config, visibility_manager).run();
        Ok(())
    }
}
