//! Configuration/layout snapshot and raw user CSS for startup or reload.
pub mod file_io;
pub mod types;
use crate::language::LanguageManager;
use anyhow::{Context, Result};
use types::ConfigFile;
pub struct AppConfig {
    language_manager: LanguageManager,
    config_file: ConfigFile,
    user_css: Option<String>,
}
impl AppConfig {
    #[cfg(test)]
    pub(crate) fn test_builtins() -> Self {
        let config_file = toml::from_str("").unwrap();
        let files = crate::layout::assets::names()
            .map(|name| {
                let asset = crate::layout::assets::get(name).unwrap();
                (name.to_string(), asset.to_owned())
            })
            .collect();
        Self {
            language_manager: LanguageManager::from_files(&config_file, files).unwrap(),
            config_file,
            user_css: None,
        }
    }

    pub fn keyboard_width(&self) -> i32 {
        self.config_file
            .keyboard_width
            .unwrap_or(crate::constants::geometry::DEFAULT_KEYBOARD_WIDTH)
    }
    pub fn stretch(&self) -> bool {
        matches!(
            self.config_file.backdrop_width.unwrap_or_default(),
            types::BackdropWidth::Stretch
        )
    }
    pub fn with_config_loaded() -> Result<Self> {
        let content = file_io::read_optional("config")?.unwrap_or_default();
        let config_file: ConfigFile = toml::from_str(&content).context("Invalid config TOML")?;
        anyhow::ensure!(
            config_file
                .keyboard_width
                .is_none_or(|width| (240..=7680).contains(&width)),
            "keyboard_width must be 240–7680"
        );
        let user_css = file_io::read_optional("style.css")?;
        let language_manager = LanguageManager::load(&config_file)?;
        Ok(Self {
            language_manager,
            config_file,
            user_css,
        })
    }
    pub fn get_user_css_override(&self) -> Option<String> {
        self.user_css.clone()
    }
    pub fn get_language_manager(&self) -> &LanguageManager {
        &self.language_manager
    }
    pub fn get_language_manager_mut(&mut self) -> &mut LanguageManager {
        &mut self.language_manager
    }
    pub fn is_dark_mode(&self) -> bool {
        self.config_file.dark_mode.unwrap_or(false)
    }
}
