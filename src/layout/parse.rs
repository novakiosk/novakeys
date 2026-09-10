use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Key {
    /// Input text before Shift handling or native composition.
    pub text: Option<String>,

    /// Alternative characters available via long press or popup
    pub alternatives: Option<Vec<String>>,

    /// Special action (backspace, enter, shift, etc.)
    pub action: Option<String>,

    /// Display text (for keys like Space that show different text than they insert)
    pub display_text: Option<String>,

    /// Language-selector label; overrides the current language flag or globe fallback.
    pub flag: Option<String>,

    /// Relative width within the row; defaults to 1.
    pub width: Option<u32>,
}

impl Key {
    pub fn get_display_text(&self) -> Cow<'_, str> {
        if let Some(display_text) = &self.display_text {
            Cow::Borrowed(display_text)
        } else if let Some(text) = &self.text {
            Cow::Borrowed(text)
        } else if let Some(action) = &self.action {
            match action.as_str() {
                "backspace" => Cow::Borrowed("⌫"),
                "enter" => Cow::Borrowed("↵"),
                "shift" => Cow::Borrowed("⇧"),
                "language_selector" => Cow::Borrowed("🌐"),
                _ => Cow::Borrowed(action),
            }
        } else {
            Cow::Borrowed("?")
        }
    }

    pub fn get_display_text_with_flag(&self, current_flag: Option<&str>) -> String {
        if let Some(action) = &self.action
            && action == "language_selector"
        {
            if let Some(flag) = &self.flag {
                return flag.clone();
            }
            return current_flag.unwrap_or("🌐").to_string();
        }

        self.get_display_text().into_owned()
    }

    pub fn get_insert_text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn get_alternatives(&self) -> &[String] {
        self.alternatives.as_deref().unwrap_or(&[])
    }

    pub fn get_width(&self) -> u32 {
        self.width.unwrap_or(1)
    }

    pub fn is_text_key(&self) -> bool {
        self.text.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayoutDefinition {
    pub default: Option<bool>,
    pub layout: Vec<Vec<Key>>,

    pub language_code: Option<String>,
    pub language_name: Option<String>,
    pub language_flag: Option<String>,

    /// Runtime row count (not serialized).
    #[serde(skip)]
    pub height: i32,
}

impl LayoutDefinition {
    pub fn parse(content: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(content.len() <= 1024 * 1024, "Layout exceeds 1 MiB");
        let mut layout: Self = toml::from_str(content)?;
        anyhow::ensure!(
            !layout.layout.is_empty() && layout.layout.len() <= 16,
            "Layout needs 1–16 rows"
        );
        for row in &layout.layout {
            anyhow::ensure!(!row.is_empty() && row.len() <= 64, "Row needs 1–64 keys");
            for key in row {
                anyhow::ensure!(
                    (1..=32).contains(&key.get_width()),
                    "Key width must be 1–32"
                );
                anyhow::ensure!(
                    key.text.as_ref().is_none_or(|s| s.len() <= 1024),
                    "Key text exceeds 1024 bytes"
                );
                anyhow::ensure!(
                    key.alternatives
                        .as_ref()
                        .is_none_or(|a| a.len() <= 8 && a.iter().all(|s| s.len() <= 1024)),
                    "Each key supports up to 8 alternatives of at most 1024 bytes"
                );
            }
        }
        layout.height = layout.layout.len() as i32;
        Ok(layout)
    }
}

pub fn shifted_text(text: &str, shift: bool, language: Option<&str>) -> String {
    if shift && text.chars().count() == 1 {
        if language == Some("ko") {
            match text {
                "ㅂ" => "ㅃ",
                "ㅈ" => "ㅉ",
                "ㄷ" => "ㄸ",
                "ㄱ" => "ㄲ",
                "ㅅ" => "ㅆ",
                "ㅐ" => "ㅒ",
                "ㅔ" => "ㅖ",
                _ => text,
            }
            .to_owned()
        } else if language == Some("tr") && text == "i" {
            "İ".to_owned()
        } else {
            text.to_uppercase()
        }
    } else {
        text.to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shipped_layouts_parse_and_shift_matches_unicode_insertion() {
        for name in crate::layout::assets::names() {
            let data = crate::layout::assets::get(name).unwrap();
            LayoutDefinition::parse(data).unwrap();
        }
        assert_eq!(shifted_text("i", true, Some("tr")), "İ");
        assert_eq!(shifted_text("ı", true, Some("tr")), "I");
        assert_eq!(shifted_text("i", true, Some("en")), "I");
        assert_eq!(shifted_text("i", false, Some("tr")), "i");
        assert_eq!(shifted_text("å", true, None), "Å");
        assert_eq!(shifted_text("æ", true, None), "Æ");
        assert_eq!(shifted_text("ø", true, None), "Ø");
        assert_eq!(shifted_text("ß", true, None), "SS");
        assert_eq!(shifted_text("two words", true, None), "two words");
    }
    #[test]
    fn bundled_character_additions_are_reachable_in_bounded_popups() {
        for (language, required) in [
            ("fr", "æœÿ"),
            ("el", "άέήίϊΐόύϋΰώς"),
            ("uk", "ґʼ’"),
            ("ka", "თღჩძ"),
            ("ar", "دآأإؤئءًٌٍَُِّْٰ"),
            ("ur", "ٹڑژےئؤء"),
            ("hi", "ऋऌऍऑपफबभमयरलवशषसहऴ्ृॅॉँः।॥"),
            ("ko", "ㄲㄸㅃㅆㅉㅒㅖㅘㅙㅚㅝㅞㅟㅢ"),
            ("zh", "，。？！：；、（）「」"),
        ] {
            let name = format!("layout-{language}.toml");
            let data = crate::layout::assets::get(&name).unwrap();
            let layout = LayoutDefinition::parse(data).unwrap();
            let outputs: Vec<&str> = layout
                .layout
                .iter()
                .flatten()
                .flat_map(|key| {
                    key.text
                        .iter()
                        .chain(key.get_alternatives())
                        .map(String::as_str)
                })
                .collect();
            for ch in required.chars() {
                assert!(
                    outputs.iter().any(|text| *text == ch.to_string()),
                    "{language} missing {ch}"
                );
            }
        }
        for name in crate::layout::assets::names() {
            let data = crate::layout::assets::get(name).unwrap();
            let layout = LayoutDefinition::parse(data).unwrap();
            assert!(
                layout
                    .layout
                    .iter()
                    .flatten()
                    .all(|key| key.get_alternatives().len() <= 8),
                "{name} popup too wide"
            );
        }
    }
    #[test]
    fn alternatives_are_bounded_to_the_popup_capacity() {
        for (count, accepted) in [(8, true), (9, false)] {
            let alternatives = vec!["'á'"; count].join(",");
            let input = format!("layout = [[{{text = 'a', alternatives = [{alternatives}]}}]]");
            assert_eq!(LayoutDefinition::parse(&input).is_ok(), accepted);
        }
    }
    #[test]
    fn rejects_empty_geometry_and_overflowing_width() {
        for content in [
            "layout = []",
            "layout = [[]]",
            "layout = [[{text = 'a', width = 0}]]",
            "layout = [[{text = 'a', width = 4294967295}]]",
        ] {
            assert!(LayoutDefinition::parse(content).is_err());
        }
    }
}
