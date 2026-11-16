# BYOB

**Bring Your Own Branches.** Choose the Git repositories for a task, prepare their worktrees, and open them together in a local desktop coding session.

## Install on macOS

Build with stable Rust and install the CLI and Finder launcher.

```sh
cargo build --release --locked
scripts/install-macos.sh target/release/byob
```

Open **New BYOB Session** from Spotlight or `~/Applications`. You can also run `~/.local/bin/byob new` in Terminal.

## Start a session

1. Choose a directory containing your Git repos.
2. Select the repos for the task and choose to create fresh worktrees or use existing checkouts.
3. Pick Claude Desktop or Codex desktop and name the workspace. When creating worktrees, choose the feature branch name and base.
4. BYOB opens the resulting folder in Finder. Choose it in Claude Desktop's **Code → New session → Local**, or as a project for a **Local** chat in Codex desktop.

New worktrees live under `~/BYOB Workspaces/<name>`. When reusing checkouts, BYOB adds links and records the actual checkout paths in `byob.json`. Attach those actual paths in your desktop client if its sandbox limits access to the workspace folder.

## Sync base branches

```sh
byob sync /path/to/repository-library
```

BYOB fetches `origin` once per repo. It fast-forwards clean checked-out `main` and `master` branches to their matching remote refs. It skips dirty or diverged base branches and reports feature worktrees' distance from `origin/HEAD` without changing those branches. Run it manually while worktree operations are idle.

## Project status

The repo picker, worktree preparation, and desktop handoff are the first milestone. DEV/PROD profiles, the local proxy and process supervisor, desktop skills, and scheduled sync are planned.
