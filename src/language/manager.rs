//! Validated immutable layout snapshot with a small mutable language selection.
use super::{discovery, types::LanguageInfo};
use crate::{config::types::ConfigFile, layout::parse::LayoutDefinition};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, sync::Arc};
pub struct LanguageManager {
    languages: BTreeMap<String, (LanguageInfo, Arc<LayoutDefinition>)>,
    current_language: String,
    show_switcher: bool,
}
impl LanguageManager {
    pub fn load(config: &ConfigFile) -> Result<Self> {
        Self::from_files(config, discovery::discover(config)?)
    }
    pub(crate) fn from_files(config: &ConfigFile, files: Vec<(String, String)>) -> Result<Self> {
        let mut languages = BTreeMap::new();
        let mut marked_default = None;
        for (filename, content) in files {
            let code = filename
                .strip_prefix("layout-")
                .and_then(|s| s.strip_suffix(".toml"))
                .context("Invalid layout filename")?
                .to_string();
            ensure!(valid_code(&code), "Invalid layout language code");
            if config
                .supported_languages
                .as_ref()
                .is_some_and(|codes| !codes.contains(&code))
            {
                continue;
            }
            let mut layout =
                LayoutDefinition::parse(&content).with_context(|| format!("Invalid {filename}"))?;
            ensure!(
                layout
                    .language_code
                    .as_ref()
                    .is_none_or(|metadata| metadata == &code),
                "Layout language_code disagrees with filename: {filename}"
            );
            layout.language_code = Some(code.clone());
            if layout.default.unwrap_or(false) {
                marked_default.get_or_insert(code.clone());
            }
            let info = LanguageInfo {
                name: layout.language_name.clone().unwrap_or_else(|| code.clone()),
                flag: layout.language_flag.clone().unwrap_or_else(|| "🌐".into()),
                code: code.clone(),
                layout_file: filename,
            };
            languages.insert(code, (info, Arc::new(layout)));
        }
        ensure!(!languages.is_empty(), "No valid enabled layouts");
        if let Some(supported) = &config.supported_languages {
            ensure!(
                !supported.is_empty() && supported.len() <= 64,
                "supported_languages needs 1–64 entries"
            );
            for code in supported {
                ensure!(
                    valid_code(code) && languages.contains_key(code),
                    "Enabled layout unavailable: {code}"
                );
            }
        }
        let current_language = config
            .default_language
            .clone()
            .or(marked_default)
            .unwrap_or_else(|| {
                if languages.contains_key("en") {
                    "en".into()
                } else {
                    languages.first_key_value().unwrap().0.clone()
                }
            });
        ensure!(
            languages.contains_key(&current_language),
            "Default language is not enabled"
        );
        Ok(Self {
            languages,
            current_language,
            show_switcher: config.show_language_switcher.unwrap_or(true),
        })
    }
    pub fn get_current_language(&self) -> &str {
        &self.current_language
    }
    pub fn set_current_language(&mut self, code: &str) {
        if self.languages.contains_key(code) {
            self.current_language = code.into();
        }
    }
    pub fn get_current_language_flag(&self) -> &str {
        &self.languages[&self.current_language].0.flag
    }
    pub fn get_available_languages(&self) -> Vec<&LanguageInfo> {
        self.languages.values().map(|(info, _)| info).collect()
    }
    pub fn should_show_language_switcher(&self) -> bool {
        self.show_switcher && self.languages.len() > 1
    }
    pub fn set_language_switcher_visibility(&mut self, show: bool) {
        self.show_switcher = show;
    }
    pub fn current_layout(&self) -> Arc<LayoutDefinition> {
        self.languages[&self.current_language].1.clone()
    }
    pub fn layout(&self, code: &str) -> Option<Arc<LayoutDefinition>> {
        self.languages.get(code).map(|(_, layout)| layout.clone())
    }
}
fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 32
        && code
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> ConfigFile {
        toml::from_str("default_language = 'en'\nsupported_languages = ['en', 'custom']").unwrap()
    }
    #[test]
    fn metadata_free_turkish_layout_uses_filename_for_casing() {
        let config = toml::from_str("default_language = 'tr'").unwrap();
        let manager = LanguageManager::from_files(
            &config,
            vec![("layout-tr.toml".into(), "layout = [[{text = 'i'}]]".into())],
        )
        .unwrap();
        let layout = manager.current_layout();
        assert_eq!(
            crate::layout::parse::shifted_text(
                layout.layout[0][0].text.as_deref().unwrap(),
                true,
                layout.language_code.as_deref()
            ),
            "İ"
        );
    }
    #[test]
    fn enabled_secondary_layout_must_be_valid_and_match_filename() {
        let good = "layout = [[{text = 'a'}]]";
        for bad in [
            "not toml",
            "layout = []",
            "language_code = 'wrong'\nlayout = [[{text = 'a'}]]",
        ] {
            assert!(
                LanguageManager::from_files(
                    &config(),
                    vec![
                        ("layout-en.toml".into(), good.into()),
                        ("layout-custom.toml".into(), bad.into())
                    ]
                )
                .is_err()
            );
        }
        assert!(
            LanguageManager::from_files(&config(), vec![("layout-en.toml".into(), good.into())])
                .is_err()
        );
        let manager = LanguageManager::from_files(
            &config(),
            vec![
                ("layout-en.toml".into(), good.into()),
                ("layout-custom.toml".into(), good.into()),
            ],
        )
        .unwrap();
        assert!(manager.layout("custom").is_some());
        assert!(manager.layout("../outside").is_none());
    }
}
