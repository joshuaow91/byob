#!/bin/sh
set -eu

if [ "$#" -ne 1 ] || [ ! -x "$1" ]; then
  printf '%s\n' 'Usage: scripts/install-macos.sh path/to/built/byob' >&2
  exit 2
fi

bin_dir="$HOME/.local/bin"
apps_dir="$HOME/Applications"
destination="$apps_dir/New BYOB Session.app"
mkdir -p "$bin_dir" "$apps_dir"
if [ -e "$destination" ] || [ -L "$destination" ] || [ -e "$bin_dir/byob" ] || [ -L "$bin_dir/byob" ]; then
  printf '%s\n' 'A BYOB installation already exists. Move it aside before installing again.' >&2
  exit 1
fi

scratch="$(mktemp -d "$apps_dir/.byob-install.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
cat > "$scratch/launcher.applescript" <<'APPLESCRIPT'
on run
    set homePath to POSIX path of (path to home folder)
    set binaryPath to homePath & ".local/bin/byob"
    try
        do shell script "PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin " & quoted form of binaryPath & " new"
    on error messageText
        display alert "BYOB could not prepare a workspace" message messageText
    end try
end run
APPLESCRIPT
/usr/bin/osacompile -o "$scratch/New BYOB Session.app" "$scratch/launcher.applescript"
install -m 755 "$1" "$bin_dir/byob"
mv "$scratch/New BYOB Session.app" "$destination"
printf 'Installed %s and %s\n' "$bin_dir/byob" "$destination"
