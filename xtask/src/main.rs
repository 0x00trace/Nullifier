//! Repository maintenance. Run with `cargo xtask <command>`.

mod policy;

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check tracked and non-ignored untracked files against the asset policy.
    Policy {
        /// Check only what is staged for commit (used by the pre-commit hook).
        #[arg(long)]
        staged: bool,
    },
    /// Point git at .githooks/ so every commit runs the policy check.
    InstallHooks,
}

fn main() -> Result<()> {
    let root = workspace_root();
    match Cli::parse().command {
        Cmd::Policy { staged } => policy::run(&root, staged),
        Cmd::InstallHooks => {
            let status = Command::new("git")
                .args(["config", "core.hooksPath", ".githooks"])
                .current_dir(&root)
                .status()
                .context("running git")?;
            anyhow::ensure!(
                status.success(),
                "git config failed (is this a git repository?)"
            );
            println!("hooks installed: commits now run `cargo xtask policy --staged`");
            Ok(())
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace")
        .into()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    /// Git runs a hook only when its file is executable, and a checkout takes
    /// that bit from the mode recorded in the index. With mode 100644, git on
    /// Linux and macOS skips the asset-policy hook and says so in a hint.
    #[test]
    fn hooks_are_executable_in_the_index() {
        let out = Command::new("git")
            .args(["ls-files", "--stage", "--", ".githooks"])
            .current_dir(crate::workspace_root())
            .output()
            .expect("running git");
        assert!(out.status.success(), "git ls-files failed");
        let listing = String::from_utf8(out.stdout).expect("git prints UTF-8");
        assert!(!listing.trim().is_empty(), ".githooks has no tracked files");
        for line in listing.lines() {
            assert!(
                line.starts_with("100755 "),
                "not executable in the index: {line}"
            );
        }
    }
}
