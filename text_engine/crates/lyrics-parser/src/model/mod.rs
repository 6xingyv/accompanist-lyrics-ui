pub mod karaoke;
pub mod synced;

use serde::{Deserialize, Serialize};

pub use karaoke::{
    AccompanimentKaraokeLine, KaraokeAlignment, KaraokeLine, KaraokeSyllable, MainKaraokeLine,
    PhoneticLevel,
};
pub use synced::{SyncedLine, UncheckedSyncedLine};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub kind: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attributes {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub offset: i32,
    pub duration: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncedLineKind {
    Synced(SyncedLine),
    MainKaraoke(MainKaraokeLine),
    AccompanimentKaraoke(AccompanimentKaraokeLine),
}

impl SyncedLineKind {
    pub fn start(&self) -> i32 {
        match self {
            Self::Synced(line) => line.start,
            Self::MainKaraoke(line) => line.start,
            Self::AccompanimentKaraoke(line) => line.start,
        }
    }

    pub fn end(&self) -> i32 {
        match self {
            Self::Synced(line) => line.end,
            Self::MainKaraoke(line) => line.end,
            Self::AccompanimentKaraoke(line) => line.end,
        }
    }

    pub fn content_string(&self) -> String {
        match self {
            Self::Synced(line) => line.content.clone(),
            Self::MainKaraoke(line) => line.syllables.iter().map(|s| s.content.as_str()).collect(),
            Self::AccompanimentKaraoke(line) => {
                line.syllables.iter().map(|s| s.content.as_str()).collect()
            }
        }
    }

    pub fn with_translation(self, translation: String) -> Self {
        match self {
            Self::Synced(mut line) => {
                line.translation = Some(translation);
                Self::Synced(line)
            }
            Self::MainKaraoke(mut line) => {
                line.translation = Some(translation);
                Self::MainKaraoke(line)
            }
            Self::AccompanimentKaraoke(mut line) => {
                line.translation = Some(translation);
                Self::AccompanimentKaraoke(line)
            }
        }
    }

    pub fn same_variant_or_translation_compatible(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
            || matches!(self, Self::AccompanimentKaraoke(_))
                && matches!(other, Self::MainKaraoke(_) | Self::Synced(_))
            || matches!(other, Self::Synced(_))
    }
}

impl From<SyncedLine> for SyncedLineKind {
    fn from(value: SyncedLine) -> Self {
        Self::Synced(value)
    }
}

impl From<MainKaraokeLine> for SyncedLineKind {
    fn from(value: MainKaraokeLine) -> Self {
        Self::MainKaraoke(value)
    }
}

impl From<AccompanimentKaraokeLine> for SyncedLineKind {
    fn from(value: AccompanimentKaraokeLine) -> Self {
        Self::AccompanimentKaraoke(value)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncedLyrics {
    pub lines: Vec<SyncedLineKind>,
    pub title: String,
    pub id: String,
    pub artists: Vec<Artist>,
}

impl SyncedLyrics {
    pub fn new(lines: Vec<SyncedLineKind>) -> Self {
        Self {
            lines,
            title: String::new(),
            id: "0".to_string(),
            artists: Vec::new(),
        }
    }

    pub fn get_current_first_highlight_line_index_by_time(&self, time: i32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }

        let mut low = 0usize;
        let mut high = self.lines.len() - 1;
        let mut result_index = self.lines.len();

        while low <= high {
            let mid = low + (high - low) / 2;
            let line = &self.lines[mid];

            if line.start() > time {
                result_index = mid;
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            } else if line.end() < time {
                low = mid + 1;
            } else {
                result_index = mid;
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        }

        if result_index < self.lines.len()
            && (self.lines[result_index].start()..=self.lines[result_index].end()).contains(&time)
        {
            result_index
        } else {
            low.min(self.lines.len())
        }
    }

    pub fn get_current_all_highlight_line_indices_by_time(&self, time: i32) -> Vec<usize> {
        if self.lines.is_empty() {
            return Vec::new();
        }

        let mut low = 0usize;
        let mut high = self.lines.len() - 1;
        let mut first_after_index = self.lines.len();

        while low <= high {
            let mid = low + (high - low) / 2;
            if self.lines[mid].start() > time {
                first_after_index = mid;
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            } else {
                low = mid + 1;
            }
        }

        let mut results = Vec::new();
        for i in (0..first_after_index).rev() {
            let line = &self.lines[i];
            if (line.start()..=line.end()).contains(&time) {
                results.push(i);
            }
        }
        results.sort_unstable();
        results
    }
}
