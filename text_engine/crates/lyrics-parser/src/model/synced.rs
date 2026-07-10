use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncedLine {
    pub content: String,
    pub translation: Option<String>,
    pub start: i32,
    pub end: i32,
}

impl SyncedLine {
    pub fn new(content: String, translation: Option<String>, start: i32, end: i32) -> Self {
        assert!(end >= start);
        Self {
            content,
            translation,
            start,
            end,
        }
    }

    pub fn duration(&self) -> i32 {
        self.end - self.start
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UncheckedSyncedLine {
    pub content: String,
    pub translation: Option<String>,
    pub start: i32,
    pub end: i32,
}

impl UncheckedSyncedLine {
    pub fn duration(&self) -> i32 {
        (self.end - self.start).max(0)
    }

    pub fn to_synced_line(self) -> SyncedLine {
        SyncedLine::new(self.content, self.translation, self.start, self.end)
    }
}
