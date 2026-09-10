//! Resolve custom layouts with XDG precedence; include built-ins only when configured or needed.
use crate::{
    config::{
        file_io::{get_xdg_dirs, read_text},
        types::ConfigFile,
    },
    layout::assets,
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
pub fn discover(config: &ConfigFile) -> Result<Vec<(String, String)>> {
    let directories: Vec<_> = get_xdg_dirs().find_config_files("").collect();
    discover_in(config, &directories)
}
fn discover_in(
    config: &ConfigFile,
    directories: &[std::path::PathBuf],
) -> Result<Vec<(String, String)>> {
    let mut custom = BTreeMap::new();
    for directory in directories {
        for entry in
            std::fs::read_dir(directory).with_context(|| format!("List {}", directory.display()))?
        {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.starts_with("layout-") && name.ends_with(".toml") {
                custom.insert(name, entry.path());
                ensure!(custom.len() <= 64, "At most 64 layouts are supported");
            }
        }
    }
    let builtin_names: Vec<_> =
        if custom.is_empty() || config.include_builtins_with_custom.unwrap_or(false) {
            assets::names().collect()
        } else {
            Vec::new()
        };
    ensure!(
        custom.len()
            + builtin_names
                .iter()
                .filter(|name| !custom.contains_key(**name))
                .count()
            <= 64,
        "At most 64 layouts are supported"
    );
    let mut files = BTreeMap::new();
    for name in builtin_names {
        if !custom.contains_key(name) {
            let asset = assets::get(name).context("Missing embedded layout")?;
            files.insert(name.to_string(), asset.to_owned());
        }
    }
    for (name, path) in custom {
        files.insert(name, read_text(&path)?);
    }
    Ok(files.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_discovery_and_user_precedence_are_real_directory_reads() {
        let system = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        std::fs::write(system.path().join("layout-custom.toml"), "system").unwrap();
        std::fs::write(user.path().join("layout-custom.toml"), "user").unwrap();
        let config: ConfigFile = toml::from_str("").unwrap();
        let files = discover_in(&config, &[system.path().into(), user.path().into()]).unwrap();
        assert_eq!(files, vec![("layout-custom.toml".into(), "user".into())]);
        std::fs::write(user.path().join("layout-custom.toml"), [0xff]).unwrap();
        assert!(discover_in(&config, &[user.path().into()]).is_err());
    }
    #[test]
    fn merged_layout_defaults_prefer_explicit_then_custom_then_english() {
        let directory = tempfile::tempdir().unwrap();
        let custom = directory.path().join("layout-zz-custom.toml");
        let config: ConfigFile = toml::from_str("include_builtins_with_custom = true").unwrap();
        let current = |config: &ConfigFile| {
            super::super::manager::LanguageManager::from_files(
                config,
                discover_in(config, &[directory.path().into()]).unwrap(),
            )
            .unwrap()
            .get_current_language()
            .to_owned()
        };
        std::fs::write(&custom, "default = true\nlayout = [[{text = 'a'}]]").unwrap();
        assert_eq!(current(&config), "zz-custom");
        let explicit = ConfigFile {
            default_language: Some("no".into()),
            ..config.clone()
        };
        assert_eq!(current(&explicit), "no");
        std::fs::write(&custom, "layout = [[{text = 'a'}]]").unwrap();
        assert_eq!(current(&config), "en");
        std::fs::remove_file(custom).unwrap();
        assert_eq!(current(&config), "en");
    }
    #[test]
    fn winning_layout_limit_is_checked_before_contents_are_loaded() {
        let directory = tempfile::tempdir().unwrap();
        let config: ConfigFile = toml::from_str("").unwrap();
        for index in 0..65 {
            std::fs::write(
                directory.path().join(format!("layout-{index}.toml")),
                [0xff],
            )
            .unwrap();
        }
        let error = discover_in(&config, &[directory.path().into()]).unwrap_err();
        assert!(error.to_string().contains("At most 64"));
        std::fs::remove_file(directory.path().join("layout-64.toml")).unwrap();
        for index in 0..64 {
            std::fs::write(
                directory.path().join(format!("layout-{index}.toml")),
                "system",
            )
            .unwrap();
        }
        let user = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("layout-0.toml"), "user").unwrap();
        let files = discover_in(&config, &[directory.path().into(), user.path().into()]).unwrap();
        assert_eq!(files.len(), 64);
        assert_eq!(
            files
                .iter()
                .find(|(name, _)| name == "layout-0.toml")
                .unwrap()
                .1,
            "user"
        );

        let merged = tempfile::tempdir().unwrap();
        let builtin_count = assets::names().count();
        for index in 0..64 - builtin_count {
            std::fs::write(
                merged.path().join(format!("layout-extra-{index}.toml")),
                "custom",
            )
            .unwrap();
        }
        std::fs::write(merged.path().join("layout-en.toml"), "override").unwrap();
        let config: ConfigFile = toml::from_str("include_builtins_with_custom = true").unwrap();
        let files = discover_in(&config, &[merged.path().into()]).unwrap();
        assert_eq!(files.len(), 64);
        assert_eq!(
            files
                .iter()
                .find(|(name, _)| name == "layout-en.toml")
                .unwrap()
                .1,
            "override"
        );
        std::fs::write(merged.path().join("layout-over-limit.toml"), [0xff]).unwrap();
        assert!(
            discover_in(&config, &[merged.path().into()])
                .unwrap_err()
                .to_string()
                .contains("At most 64")
        );
    }
}
