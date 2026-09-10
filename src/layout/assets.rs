//! Built-in layouts: adding a shipped layout requires an entry below.
macro_rules! layouts {
    ($($code:literal),+ $(,)?) => {
        const LAYOUTS: &[(&str, &str)] = &[$(
            (concat!("layout-", $code, ".toml"),
             include_str!(concat!("../../assets/layout-", $code, ".toml"))),
        )+];
    };
}
layouts!(
    "ar", "cs", "de", "el", "en", "es", "fr", "he", "hi", "hu", "ja", "ka", "ko", "no", "pl", "pt",
    "sv", "tr", "uk", "ur", "zh"
);
pub fn names() -> impl Iterator<Item = &'static str> {
    LAYOUTS.iter().map(|(name, _)| *name)
}
pub fn get(name: &str) -> Option<&'static str> {
    LAYOUTS
        .iter()
        .find_map(|(key, text)| (*key == name).then_some(*text))
}
#[cfg(test)]
mod tests {
    #[test]
    fn manifest_contains_every_shipped_layout() {
        let on_disk: std::collections::BTreeSet<_> =
            std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .filter(|name| name.starts_with("layout-") && name.ends_with(".toml"))
                .collect();
        assert_eq!(on_disk, super::names().map(str::to_owned).collect());
    }
}
