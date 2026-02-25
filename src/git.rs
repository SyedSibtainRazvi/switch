use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct ContextScope {
    pub repo_path: String,
    pub branch: String,
    pub commit_sha: String,
    pub used_repo_fallback: bool,
    pub used_branch_fallback: bool,
    pub used_commit_fallback: bool,
}

pub fn detect_scope(
    repo_override: Option<&str>,
    branch_override: Option<&str>,
) -> Result<ContextScope> {
    let (repo_path, used_repo_fallback) = match repo_override {
        Some(r) => (r.to_string(), false),
        None => match git_repo_root() {
            Some(r) => (r, false),
            None => (current_dir_fallback()?, true),
        },
    };

    let (branch, used_branch_fallback) = match branch_override {
        Some(b) => (b.to_string(), false),
        None => match git_value(["rev-parse", "--abbrev-ref", "HEAD"]) {
            Some(b) => (b, false),
            None => ("unknown".to_string(), true),
        },
    };

    let commit_from_git = match (repo_override, branch_override) {
        (Some(r), Some(b)) => git_value_in(r, ["rev-parse", b]),
        (None, Some(b)) => git_value(["rev-parse", b]),
        (Some(r), None) => git_value_in(r, ["rev-parse", "HEAD"]),
        (None, None) => git_value(["rev-parse", "HEAD"]),
    };
    let used_commit_fallback = commit_from_git.is_none();
    let commit_sha = commit_from_git.unwrap_or_else(|| "unknown".to_string());

    Ok(ContextScope {
        repo_path,
        branch,
        commit_sha,
        used_repo_fallback,
        used_branch_fallback,
        used_commit_fallback,
    })
}

pub fn warn_scope_fallback(scope: &ContextScope) {
    let mut reasons = Vec::new();
    if scope.used_repo_fallback {
        reasons.push("repo_path from current directory");
    }
    if scope.used_branch_fallback {
        reasons.push("branch set to 'unknown'");
    }
    if scope.used_commit_fallback {
        reasons.push("commit set to 'unknown'");
    }
    if reasons.is_empty() {
        return;
    }

    eprintln!(
        "warning: using fallback git scope ({}) for repo='{}', branch='{}'",
        reasons.join(", "),
        scope.repo_path,
        scope.branch
    );
}

fn git_repo_root() -> Option<String> {
    git_value(["rev-parse", "--show-toplevel"])
}

fn git_value<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let trimmed = s.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn git_value_in<const N: usize>(dir: &str, args: [&str; N]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let trimmed = s.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn current_dir_fallback() -> Result<String> {
    let cwd = std::env::current_dir().context("failed to read current dir")?;
    Ok(cwd.to_string_lossy().to_string())
}
