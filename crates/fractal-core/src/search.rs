//! Search queries shared by in-file find and project-wide search.

use regex::{Regex, RegexBuilder};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

impl SearchQuery {
    pub fn compile(&self) -> Option<Regex> {
        if self.text.is_empty() {
            return None;
        }
        let mut pat = if self.regex { self.text.clone() } else { regex::escape(&self.text) };
        if self.whole_word {
            pat = format!(r"\b(?:{pat})\b");
        }
        RegexBuilder::new(&pat).case_insensitive(!self.case_sensitive).multi_line(true).build().ok()
    }
}
