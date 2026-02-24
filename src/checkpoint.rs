use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Checkpoint {
    pub id: i64,
    pub repo_path: String,
    pub branch: String,
    pub commit_sha: String,
    pub session_id: Option<String>,
    pub done_text: Option<String>,
    pub next_text: Option<String>,
    pub blockers_text: Option<String>,
    pub tests_text: Option<String>,
    pub files: Vec<String>,
    pub created_at_ms: i64,
}

pub struct CheckpointPayload {
    pub done: Option<String>,
    pub next: Option<String>,
    pub blockers: Option<String>,
    pub tests: Option<String>,
    pub files: Vec<String>,
    pub session_id: Option<String>,
}

impl CheckpointPayload {
    pub fn is_empty(&self) -> bool {
        self.done.is_none()
            && self.next.is_none()
            && self.blockers.is_none()
            && self.tests.is_none()
            && self.files.is_empty()
    }
}

pub fn print_checkpoint(c: &Checkpoint) {
    println!("repo: {}", c.repo_path);
    println!("branch: {}", c.branch);
    println!("commit: {}", c.commit_sha);
    println!("at: {}", format_ts(c.created_at_ms));
    if let Some(session_id) = &c.session_id {
        println!("session: {session_id}");
    }
    if let Some(done) = &c.done_text {
        println!("done: {done}");
    }
    if let Some(next) = &c.next_text {
        println!("next: {next}");
    }
    if let Some(blockers) = &c.blockers_text {
        println!("blockers: {blockers}");
    }
    if let Some(tests) = &c.tests_text {
        println!("tests: {tests}");
    }
    if !c.files.is_empty() {
        println!("files: {}", c.files.join(", "));
    }
}

pub fn print_checkpoint_compact(c: &Checkpoint) {
    let done = truncate_for_log(c.done_text.as_deref().unwrap_or("-"), 96);
    let next = truncate_for_log(c.next_text.as_deref().unwrap_or("-"), 96);
    println!(
        "#{} [{}] done={} | next={}",
        c.id,
        format_ts(c.created_at_ms),
        done,
        next
    );
}

pub fn format_ts(ms: i64) -> String {
    match DateTime::<Utc>::from_timestamp_millis(ms) {
        Some(ts) => ts.to_rfc3339(),
        None => ms.to_string(),
    }
}

pub fn truncate_for_log(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let prefix: String = input.chars().take(max_chars - 3).collect();
    format!("{prefix}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_for_log_applies_ellipsis() {
        assert_eq!(truncate_for_log("short", 10), "short");
        assert_eq!(
            truncate_for_log("abcdefghijklmnopqrstuvwxyz", 8),
            "abcde..."
        );
        assert_eq!(truncate_for_log("abcdef", 3), "...");
    }
}
