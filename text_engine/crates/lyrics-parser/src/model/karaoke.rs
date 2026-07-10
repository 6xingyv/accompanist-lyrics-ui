use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KaraokeAlignment {
    Start,
    End,
    #[default]
    Unspecified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhoneticLevel {
    Line,
    Syllable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KaraokeSyllable {
    pub content: String,
    pub start: i32,
    pub end: i32,
    pub phonetic: Option<String>,
}

impl KaraokeSyllable {
    pub fn new(content: String, start: i32, end: i32) -> Self {
        assert!(end >= start);
        Self {
            content,
            start,
            end,
            phonetic: None,
        }
    }

    pub fn with_phonetic(mut self, phonetic: Option<String>) -> Self {
        self.phonetic = phonetic;
        self
    }

    pub fn duration(&self) -> i32 {
        self.end - self.start
    }

    pub fn progress(&self, current: i32) -> f32 {
        match current {
            value if value < self.start => 0.0,
            value if (self.start..=self.end).contains(&value) => {
                let duration = self.duration();
                if duration <= 0 {
                    1.0
                } else {
                    (value - self.start) as f32 / duration as f32
                }
            }
            _ => 1.0,
        }
        .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainKaraokeLine {
    pub syllables: Vec<KaraokeSyllable>,
    pub translation: Option<String>,
    pub alignment: KaraokeAlignment,
    pub start: i32,
    pub end: i32,
    pub phonetic: Option<String>,
    pub accompaniment_lines: Option<Vec<AccompanimentKaraokeLine>>,
}

impl MainKaraokeLine {
    pub fn new(
        syllables: Vec<KaraokeSyllable>,
        translation: Option<String>,
        alignment: KaraokeAlignment,
        start: i32,
        end: i32,
    ) -> Self {
        assert!(end >= start);
        Self {
            syllables,
            translation,
            alignment,
            start,
            end,
            phonetic: None,
            accompaniment_lines: None,
        }
    }

    pub fn duration(&self) -> i32 {
        self.end - self.start
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccompanimentKaraokeLine {
    pub syllables: Vec<KaraokeSyllable>,
    pub translation: Option<String>,
    pub alignment: KaraokeAlignment,
    pub start: i32,
    pub end: i32,
    pub phonetic: Option<String>,
}

impl AccompanimentKaraokeLine {
    pub fn new(
        syllables: Vec<KaraokeSyllable>,
        translation: Option<String>,
        alignment: KaraokeAlignment,
        start: i32,
        end: i32,
    ) -> Self {
        assert!(end >= start);
        Self {
            syllables,
            translation,
            alignment,
            start,
            end,
            phonetic: None,
        }
    }

    pub fn duration(&self) -> i32 {
        self.end - self.start
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KaraokeLine {
    Main(MainKaraokeLine),
    Accompaniment(AccompanimentKaraokeLine),
}

pub fn content_to_string(syllables: &[KaraokeSyllable]) -> String {
    syllables
        .iter()
        .map(|syllable| syllable.content.as_str())
        .collect()
}

pub fn phonetic_to_string(syllables: &[KaraokeSyllable]) -> String {
    syllables
        .iter()
        .map(|syllable| syllable.phonetic.as_deref().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_open_paren(ch: char) -> bool {
    ch == '(' || ch == '（'
}

fn is_close_paren(ch: char) -> bool {
    ch == ')' || ch == '）'
}

fn is_wrapped_in_matched_parens(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 || !is_open_paren(chars[0]) || !is_close_paren(chars[chars.len() - 1]) {
        return false;
    }
    let mut depth = 0i32;
    for (index, ch) in chars.iter().enumerate() {
        if is_open_paren(*ch) {
            depth += 1;
        } else if is_close_paren(*ch) {
            depth -= 1;
            if depth == 0 {
                return index == chars.len() - 1;
            }
            if depth < 0 {
                return false;
            }
        }
    }
    false
}

pub fn strip_enclosing_parentheses_text(text: &str) -> String {
    let mut current = text.to_string();
    while !current.is_empty() {
        let chars: Vec<char> = current.chars().collect();
        let Some(open) = chars.iter().position(|ch| !ch.is_whitespace()) else {
            break;
        };
        let Some(close) = chars.iter().rposition(|ch| !ch.is_whitespace()) else {
            break;
        };
        if open >= close
            || !is_wrapped_in_matched_parens(&chars[open..=close].iter().collect::<String>())
        {
            break;
        }
        let mut rebuilt = String::new();
        for (index, ch) in chars.into_iter().enumerate() {
            if index != open && index != close {
                rebuilt.push(ch);
            }
        }
        current = rebuilt;
    }
    current
}

pub fn strip_enclosing_parentheses(mut syllables: Vec<KaraokeSyllable>) -> Vec<KaraokeSyllable> {
    while !syllables.is_empty() {
        let joined = content_to_string(&syllables);
        let chars: Vec<char> = joined.chars().collect();
        let Some(open) = chars.iter().position(|ch| !ch.is_whitespace()) else {
            break;
        };
        let Some(close) = chars.iter().rposition(|ch| !ch.is_whitespace()) else {
            break;
        };
        if open >= close
            || !is_wrapped_in_matched_parens(&chars[open..=close].iter().collect::<String>())
        {
            break;
        }
        syllables = remove_chars_at_global_offsets(&syllables, &[open, close]);
    }
    syllables
}

fn remove_chars_at_global_offsets(
    syllables: &[KaraokeSyllable],
    offsets: &[usize],
) -> Vec<KaraokeSyllable> {
    let mut base = 0usize;
    syllables
        .iter()
        .map(|syllable| {
            let mut content = String::new();
            for (index, ch) in syllable.content.chars().enumerate() {
                if !offsets.contains(&(base + index)) {
                    content.push(ch);
                }
            }
            base += syllable.content.chars().count();
            KaraokeSyllable {
                content,
                ..syllable.clone()
            }
        })
        .collect()
}
