pub mod client;
pub mod host;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Unified command kind used by both CLI and IPC payloads.
#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum)]
pub enum CommandKind {
    Close,
    ReloadConfig,
    SetLanguage,
    GetStatus,
    HideLanguageSwitcher,
    ShowLanguageSwitcher,
}

/// Only SetLanguage accepts a value (the language code).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IPCMessage {
    pub version: u8,
    pub kind: CommandKind,
    pub value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct IPCResponse {
    pub error: Option<String>,
    pub status: Option<serde_json::Value>,
}
impl IPCResponse {
    pub fn error(error: String) -> Self {
        Self {
            error: Some(error),
            status: None,
        }
    }
}

impl IPCMessage {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.version == 1, "Unsupported IPC version");
        match self.kind {
            CommandKind::SetLanguage => {
                let code = self
                    .value
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Missing language code"))?;
                anyhow::ensure!(
                    !code.is_empty()
                        && code.len() <= 32
                        && code
                            .bytes()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
                    "Invalid language code"
                );
            }
            _ => anyhow::ensure!(self.value.is_none(), "Command does not accept a value"),
        }
        Ok(())
    }
}
