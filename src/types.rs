use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageCode(String);

impl fmt::Display for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for LanguageCode {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for LanguageCode {
    fn from(code: String) -> Self {
        Self(code)
    }
}

impl From<&str> for LanguageCode {
    fn from(code: &str) -> Self {
        Self(code.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowIndex(pub usize);

impl RowIndex {
    pub fn get(&self) -> usize {
        self.0
    }
}

impl From<usize> for RowIndex {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyIndex(pub usize);

impl KeyIndex {
    pub fn get(&self) -> usize {
        self.0
    }
}

impl From<usize> for KeyIndex {
    fn from(index: usize) -> Self {
        Self(index)
    }
}
