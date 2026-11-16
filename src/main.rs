mod git;
mod picker;

use serde::Serialize;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize)]
struct Repository {
    name: String,
    path: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("byob: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [action] if action == "new" => {
            if let Some(root) = picker::folder()? {
                new_workspace(&root)?;
            }
        }
        [action, root] if action == "new" => new_workspace(Path::new(root))?,
        [action, root] if action == "sync" => git::sync(Path::new(root))?,
        [] => print_usage(),
        [action] if action == "--help" || action == "-h" => print_usage(),
        _ => {
            print_usage();
            return Err("Invalid arguments".into());
        }
    }
    Ok(())
}

fn print_usage() {
    println!(
        "BYOB · Bring Your Own Branches\n\n  byob new [repo-directory]\n  byob sync <repo-directory>"
    );
}

fn new_workspace(root: &Path) -> Result<()> {
    let repos = git::discover(root)?;
    if repos.is_empty() {
        return Err(format!("No Git repositories in {}", root.display()).into());
    }
    let labels: Vec<_> = repos.iter().map(|p| repo_name(p)).collect::<Result<_>>()?;
    let pick = picker::choices("Choose the repositories for this chat.", &labels, true)?;
    if pick.is_empty() {
        return Ok(());
    }
    let Some(mode) = choose(
        "Choose how BYOB should prepare these repositories.",
        &["Create fresh worktrees", "Choose existing worktrees"],
    )?
    else {
        return Ok(());
    };
    let Some(app) = choose(
        "Choose a desktop app.",
        &["Claude Desktop", "Codex desktop"],
    )?
    else {
        return Ok(());
    };
    let selected: Vec<_> = pick
        .iter()
        .map(|index| (labels[*index].clone(), repos[*index].clone()))
        .collect();

    let Some(name) = picker::text(
        "Name this workspace. Use letters, numbers, hyphens, or underscores.",
        "",
    )?
    else {
        return Ok(());
    };
    let name = name.trim();
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(
            "Use only letters, numbers, hyphens, and underscores for the workspace name".into(),
        );
    }
    let home = PathBuf::from(env::var_os("HOME").ok_or("Missing HOME directory")?);
    let workspace_root = home.join("BYOB Workspaces");
    let workspace = workspace_root.join(name);
    if workspace.symlink_metadata().is_ok() {
        return Err(format!(
            "{} already exists. Open it in your desktop app or choose another name.",
            workspace.display()
        )
        .into());
    }

    let mut plans = Vec::new();
    if mode == 0 {
        let Some(branch) = picker::text(
            "Branch name for the new worktrees.",
            &format!("feature/{name}"),
        )?
        else {
            return Ok(());
        };
        for (name, repo) in selected {
            git::git(&repo, &["check-ref-format", "--branch", &branch])?;
            if git::git(
                &repo,
                &[
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ],
            )
            .is_ok()
            {
                return Err(format!(
                    "{name} already has {branch}. Choose an existing worktree or a new branch name."
                )
                .into());
            }
            git::git(&repo, &["fetch", "origin"])?;
            let mut bases = Vec::new();
            for base in ["origin/main", "origin/master"] {
                if git::git(
                    &repo,
                    &["rev-parse", "--verify", &format!("{base}^{{commit}}")],
                )
                .is_ok()
                {
                    bases.push(base);
                }
            }
            if bases.is_empty() {
                return Err(
                    format!("Couldn't find origin/main or origin/master for {name}").into(),
                );
            }
            let Some(base) = choose(&format!("Choose a starting base for {name}."), &bases)? else {
                return Ok(());
            };
            plans.push((name, repo, Some((branch.clone(), bases[base].to_owned()))));
        }
    } else {
        for (name, repo) in selected {
            let trees: Vec<_> = git::worktrees(&repo)?
                .into_iter()
                .filter(|w| !w.bare && w.path.is_dir())
                .collect();
            if trees.is_empty() {
                return Err(format!("No available checkouts for {name}").into());
            }
            let labels: Vec<_> = trees
                .iter()
                .map(|w| {
                    format!(
                        "{} · {}",
                        w.branch.as_deref().unwrap_or("detached HEAD"),
                        w.path.display()
                    )
                })
                .collect();
            let Some(index) = picker::choices(
                &format!("Choose the {name} checkout for this chat."),
                &labels,
                false,
            )?
            .first()
            .copied() else {
                return Ok(());
            };
            plans.push((name, trees[index].path.clone(), None));
        }
    }

    fs::create_dir_all(&workspace_root)?;
    fs::create_dir(&workspace)?;
    let mut repos = Vec::new();
    for (name, source, create) in plans {
        let link = workspace.join(&name);
        let result = if let Some((branch, base)) = create {
            let path = link.to_str().ok_or("Workspace path is not valid UTF-8")?;
            git::git(
                &source,
                &["worktree", "add", "-b", &branch, "--", path, &base],
            )
            .map(|_| link)
        } else {
            symlink(&source, &link).map(|_| source).map_err(Into::into)
        };
        let path = result.map_err(|error| {
            format!(
                "Couldn't prepare {name}: {error}. Any completed checkouts are at {}",
                workspace.display()
            )
        })?;
        repos.push(Repository { name, path });
    }
    let mut manifest = serde_json::to_string_pretty(&repos)?;
    manifest.push('\n');
    fs::write(workspace.join("byob.json"), manifest)?;
    handoff(&workspace, &repos, app, mode == 1)
}

fn repo_name(path: &Path) -> Result<String> {
    Ok(path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or("Repository name is not valid UTF-8")?
        .to_owned())
}

fn choose(prompt: &str, options: &[&str]) -> Result<Option<usize>> {
    if options.len() == 1 {
        return Ok(Some(0));
    }
    let labels: Vec<_> = options.iter().map(|s| (*s).to_owned()).collect();
    Ok(picker::choices(prompt, &labels, false)?.first().copied())
}

fn handoff(workspace: &Path, repos: &[Repository], app: usize, reused: bool) -> Result<()> {
    println!("Workspace: {}", workspace.display());
    for repo in repos {
        println!("  {}: {}", repo.name, repo.path.display());
    }
    let help = if app == 0 {
        "In Claude Desktop, choose Code → New session → Local."
    } else {
        "In Codex desktop, add a local project and start a Local chat."
    };
    let detail = if reused {
        "For folder permissions, attach the actual checkout paths listed in byob.json."
    } else {
        "Use this folder as the chat’s project folder. BYOB already created the worktrees."
    };
    picker::open(&[workspace.as_os_str()])?;
    picker::message(
        "BYOB workspace ready",
        &format!("{help}\n\n{detail}\n\n{}", workspace.display()),
    )
}
