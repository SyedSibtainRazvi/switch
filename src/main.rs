mod checkpoint;
mod db;
mod git;
mod mcp;

use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use checkpoint::{print_checkpoint, print_checkpoint_compact, CheckpointPayload};
use db::{
    delete_checkpoints_for_scope, latest_checkpoint_for_scope, list_checkpoints_for_scope, open_db,
    save_checkpoint,
};
use git::{detect_scope, warn_scope_fallback};
use mcp::run_mcp_server;

#[derive(Debug, Parser)]
#[command(
    name = "context0",
    version,
    about = "Local-first context broker for coding agents"
)]
struct Cli {
    /// Override sqlite db path (default: ~/.switch/switch.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Override git repo path detection
    #[arg(long, global = true)]
    repo: Option<String>,

    /// Override git branch detection
    #[arg(long, global = true)]
    branch: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Ensure db exists and migrations are applied
    Init,
    /// Save a checkpoint for current repo + branch
    Save {
        #[arg(long)]
        done: Option<String>,
        #[arg(long)]
        next: Option<String>,
        #[arg(long)]
        blockers: Option<String>,
        #[arg(long)]
        tests: Option<String>,
        #[arg(long, action = ArgAction::Append)]
        files: Vec<String>,
        #[arg(long)]
        session: Option<String>,
    },
    /// Show latest checkpoint for current repo + branch
    Resume {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show recent checkpoints for current repo + branch
    Log {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Delete all checkpoints for current repo + branch
    Clear,
    /// Run MCP stdio server for editor/agent integration
    McpServer,
    /// Install agent rule files into the current project (Claude Code, Cursor, Codex)
    InitRules,
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.unwrap_or_else(default_db_path);

    if let Commands::Completions { shell } = cli.command {
        clap_complete::generate(
            shell,
            &mut Cli::command(),
            "context0",
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    if let Commands::InitRules = cli.command {
        return init_rules();
    }

    let conn = open_db(&db_path)?;

    match cli.command {
        Commands::Init => {
            println!("Initialized database at {}", db_path.display());
        }
        Commands::Save {
            done,
            next,
            blockers,
            tests,
            files,
            session,
        } => {
            let scope = detect_scope(cli.repo.as_deref(), cli.branch.as_deref())?;
            warn_scope_fallback(&scope);
            let payload = CheckpointPayload {
                done,
                next,
                blockers,
                tests,
                files,
                session_id: session,
            };
            save_checkpoint(&conn, &scope, &payload)?;
            println!("Checkpoint saved");
        }
        Commands::Resume { json } => {
            let scope = detect_scope(cli.repo.as_deref(), cli.branch.as_deref())?;
            warn_scope_fallback(&scope);
            if let Some(checkpoint) =
                latest_checkpoint_for_scope(&conn, &scope.repo_path, &scope.branch)?
            {
                if json {
                    println!("{}", serde_json::to_string_pretty(&checkpoint)?);
                } else {
                    print_checkpoint(&checkpoint);
                }
            } else {
                println!("No context found for this repo/branch.");
            }
        }
        Commands::Log { limit } => {
            let scope = detect_scope(cli.repo.as_deref(), cli.branch.as_deref())?;
            warn_scope_fallback(&scope);
            let rows = list_checkpoints_for_scope(&conn, &scope.repo_path, &scope.branch, limit)?;
            if rows.is_empty() {
                println!("No checkpoints found.");
            } else {
                for row in rows {
                    print_checkpoint_compact(&row);
                }
            }
        }
        Commands::Clear => {
            let scope = detect_scope(cli.repo.as_deref(), cli.branch.as_deref())?;
            warn_scope_fallback(&scope);
            let count = delete_checkpoints_for_scope(&conn, &scope.repo_path, &scope.branch)?;
            println!("Deleted {count} checkpoint(s)");
        }
        Commands::McpServer => {
            run_mcp_server(&conn)?;
        }
        Commands::InitRules | Commands::Completions { .. } => unreachable!(),
    }

    Ok(())
}

const RULE_MARKER: &str = "context0 context handoff";
const CLAUDE_RULE: &str = include_str!("../rules/CLAUDE.md");
const CURSOR_RULE: &str = include_str!("../rules/context0.mdc");
const AGENTS_RULE: &str = include_str!("../rules/AGENTS.md");

fn init_rules() -> Result<()> {
    println!("Installing context0 rule files...\n");

    // Claude Code — append to CLAUDE.md at project root
    write_rule_append(Path::new("CLAUDE.md"), CLAUDE_RULE, "Claude Code")?;

    // Cursor — dedicated file, always write
    fs::create_dir_all(".cursor/rules")?;
    write_rule_overwrite(
        Path::new(".cursor/rules/context0.mdc"),
        CURSOR_RULE,
        "Cursor",
    )?;

    // Codex — append to AGENTS.md at project root
    write_rule_append(Path::new("AGENTS.md"), AGENTS_RULE, "Codex")?;

    println!("\nDone. Configure MCP in each tool, then agents will save and resume context automatically.");
    println!("See README for MCP config snippets.");
    Ok(())
}

fn write_rule_append(path: &Path, content: &str, tool: &str) -> Result<()> {
    if path.exists() {
        let existing = fs::read_to_string(path)?;
        if existing.contains(RULE_MARKER) {
            println!(
                "  skipped   {} — already installed ({})",
                path.display(),
                tool
            );
            return Ok(());
        }
        let mut file = fs::OpenOptions::new().append(true).open(path)?;
        write!(file, "\n{content}")?;
        println!("  appended  {} ({})", path.display(), tool);
    } else {
        fs::write(path, content)?;
        println!("  created   {} ({})", path.display(), tool);
    }
    Ok(())
}

fn write_rule_overwrite(path: &Path, content: &str, tool: &str) -> Result<()> {
    fs::write(path, content)?;
    println!("  created   {} ({})", path.display(), tool);
    Ok(())
}

fn default_db_path() -> PathBuf {
    match dirs::home_dir() {
        Some(home) => home.join(".context0").join("context0.db"),
        None => PathBuf::from(".context0/context0.db"),
    }
}
