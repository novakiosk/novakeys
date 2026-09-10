use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub keyboard_width: Option<i32>,
    pub backdrop_width: Option<BackdropWidth>,
    pub default_language: Option<String>,

    /// Omission enables all discovered layouts, including custom layouts.
    pub supported_languages: Option<Vec<String>>,

    /// Whether to show the language switcher (default: true)
    pub show_language_switcher: Option<bool>,

    /// Adds the novakeys-dark CSS class; defaults to false.
    pub dark_mode: Option<bool>,

    /// Defaults to custom layouts only when present; true also includes built-ins.
    pub include_builtins_with_custom: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackdropWidth {
    #[default]
    Content,
    Stretch,
}
