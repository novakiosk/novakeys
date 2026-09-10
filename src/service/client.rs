use crate::{
    cli::ProgramArgs,
    ipc::Ipc,
    service::{CommandKind, IPCMessage},
};
use anyhow::{Context, Result};
pub struct MessageService {
    ipc_handle: Ipc,
    args: ProgramArgs,
}
impl MessageService {
    pub fn new(ipc_handle: Ipc, args: ProgramArgs) -> Self {
        Self { ipc_handle, args }
    }
    pub fn run(&self) -> Result<()> {
        for kind in &self.args.message {
            let value = if matches!(kind, CommandKind::SetLanguage) {
                Some(
                    self.args
                        .value
                        .clone()
                        .context("set-language requires a language code")?,
                )
            } else {
                None
            };
            let response = self.ipc_handle.send(&IPCMessage {
                version: 1,
                kind: kind.clone(),
                value,
            })?;
            if let Some(status) = response.status {
                println!("{}", serde_json::to_string_pretty(&status)?);
            }
        }
        Ok(())
    }
}
