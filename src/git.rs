use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub bare: bool,
}

pub fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "{}: git {}: {}",
            repo.display(),
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub fn discover(root: &Path) -> Result<Vec<PathBuf>> {
    let mut repos = Vec::new();
    for entry in root.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if path.join(".git").is_dir() {
            repos.push(path.canonicalize()?);
        }
    }
    repos.sort();
    repos.dedup();
    Ok(repos)
}

pub fn worktrees(repo: &Path) -> Result<Vec<Worktree>> {
    let raw = git(repo, &["worktree", "list", "--porcelain", "-z"])?;
    Ok(parse_worktrees(&raw))
}

fn parse_worktrees(raw: &str) -> Vec<Worktree> {
    let mut trees: Vec<Worktree> = Vec::new();
    for field in raw.split('\0') {
        if let Some(path) = field.strip_prefix("worktree ") {
            trees.push(Worktree {
                path: path.into(),
                branch: None,
                bare: false,
            });
        } else if let Some(tree) = trees.last_mut() {
            if let Some(branch) = field.strip_prefix("branch refs/heads/") {
                tree.branch = Some(branch.to_owned());
            } else if field == "bare" {
                tree.bare = true;
            }
        }
    }
    trees
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_paths_keep_spaces_and_detached_checkouts_have_no_branch() {
        let trees = parse_worktrees(concat!(
            "worktree /tmp/Main checkout\0HEAD 123\0branch refs/heads/main\0\0",
            "worktree /tmp/Another checkout\0HEAD 456\0detached\0\0",
        ));
        assert_eq!(trees.len(), 2);
        assert_eq!(trees[0].path, Path::new("/tmp/Main checkout"));
        assert_eq!(trees[0].branch.as_deref(), Some("main"));
        assert_eq!(trees[1].path, Path::new("/tmp/Another checkout"));
        assert!(trees[1].branch.is_none());
    }
}

pub fn sync(root: &Path) -> Result<()> {
    let repos = discover(root)?;
    if repos.is_empty() {
        return Err(format!("No Git repositories in {}", root.display()).into());
    }
    let mut failures = 0;
    for repo in repos {
        if let Err(error) = sync_repo(&repo) {
            eprintln!("{error}");
            failures += 1;
        }
    }
    if failures > 0 {
        return Err(format!("Sync could not complete for {failures} repositories").into());
    }
    Ok(())
}

fn sync_repo(repo: &Path) -> Result<()> {
    git(repo, &["fetch", "origin"])?;
    println!("Fetched {}", repo.display());
    for tree in worktrees(repo)? {
        if tree.bare || !tree.path.is_dir() {
            continue;
        }
        let Some(branch) = tree.branch else {
            println!("  {}: detached HEAD, left as-is", tree.path.display());
            continue;
        };
        let upstream = if branch == "main" || branch == "master" {
            format!("refs/remotes/origin/{branch}")
        } else {
            "refs/remotes/origin/HEAD".to_owned()
        };
        if git(repo, &["rev-parse", "--verify", &upstream]).is_err() {
            println!("  {}: no {}, skipped", tree.path.display(), upstream);
            continue;
        }
        let range = format!("HEAD...{upstream}");
        let counts = git(&tree.path, &["rev-list", "--left-right", "--count", &range])?;
        let (ahead, behind) = counts
            .trim()
            .split_once('\t')
            .ok_or("Unexpected Git count")?;
        if branch != "main" && branch != "master" {
            println!(
                "  {} ({branch}): {behind} behind {upstream}",
                tree.path.display()
            );
            continue;
        }
        if behind == "0" {
            println!(
                "  {} ({branch}): {ahead} ahead, 0 behind",
                tree.path.display()
            );
            continue;
        }
        if ahead != "0"
            || !git(
                &tree.path,
                &["status", "--porcelain", "--untracked-files=all"],
            )?
            .is_empty()
        {
            println!(
                "  {} ({branch}): diverged or dirty, skipped",
                tree.path.display()
            );
            continue;
        }
        let current = git(&tree.path, &["symbolic-ref", "--short", "HEAD"])?;
        if current.trim_end() != branch {
            println!(
                "  {}: branch changed during sync, skipped",
                tree.path.display()
            );
            continue;
        }
        git(&tree.path, &["merge", "--ff-only", &upstream])?;
        println!("  {} ({branch}): fast-forwarded", tree.path.display());
    }
    Ok(())
}
