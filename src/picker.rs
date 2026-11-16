use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// Arguments are data. Keep paths and repo names out of the AppleScript source.
fn dialog(script: &str, args: &[&str]) -> Result<String> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing AppleScript stdin")?
        .write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_owned()
            .into());
    }
    let mut text = String::new();
    output.stdout.as_slice().read_to_string(&mut text)?;
    if text.ends_with('\n') {
        text.pop();
    }
    Ok(text)
}

pub fn folder() -> Result<Option<PathBuf>> {
    let path = dialog(
        r#"
        on run argv
            try
                return POSIX path of (choose folder with prompt "Choose the folder that contains your Git repositories")
            on error number -128
                return ""
            end try
        end run
    "#,
        &[],
    )?;
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}

pub fn choices(prompt: &str, labels: &[String], multiple: bool) -> Result<Vec<usize>> {
    let choices: Vec<String> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| format!("{}. {label}", i + 1))
        .collect();
    let mut args = vec![prompt, if multiple { "yes" } else { "no" }];
    args.extend(choices.iter().map(String::as_str));
    let result = dialog(
        r#"
        on run argv
            set entries to items 3 thru -1 of argv
            if (item 2 of argv) is "yes" then
                set picked to choose from list entries with title "BYOB" with prompt (item 1 of argv) with multiple selections allowed
            else
                set picked to choose from list entries with title "BYOB" with prompt (item 1 of argv)
            end if
            if picked is false then return ""
            set pickedIndexes to {}
            repeat with rowNumber from 1 to (count entries)
                if picked contains (item rowNumber of entries) then set end of pickedIndexes to rowNumber
            end repeat
            set AppleScript's text item delimiters to ","
            return pickedIndexes as text
        end run
    "#,
        &args,
    )?;
    if result.is_empty() {
        return Ok(Vec::new());
    }
    result
        .split(',')
        .map(|s| {
            let index = s.parse::<usize>()?;
            if index == 0 || index > labels.len() {
                return Err("Picker returned an invalid index".into());
            }
            Ok(index - 1)
        })
        .collect()
}

pub fn text(prompt: &str, default: &str) -> Result<Option<String>> {
    let response = dialog(
        r#"
        on run argv
            try
                return text returned of (display dialog (item 1 of argv) with title "BYOB" default answer (item 2 of argv))
            on error number -128
                return ""
            end try
        end run
    "#,
        &[prompt, default],
    )?;
    Ok((!response.is_empty()).then_some(response))
}

pub fn message(title: &str, body: &str) -> Result<()> {
    dialog(r#"
        on run argv
            display dialog (item 2 of argv) with title (item 1 of argv) buttons {"OK"} default button "OK"
        end run
    "#, &[title, body]).map(|_| ())
}

pub fn open(args: &[&std::ffi::OsStr]) -> io::Result<()> {
    if Command::new("/usr/bin/open").args(args).status()?.success() {
        Ok(())
    } else {
        Err(io::Error::other("macOS could not open the selection"))
    }
}
