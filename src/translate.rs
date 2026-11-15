//! Glue: a raw command line in, a [`Translation`] out.
//!
//! Lines are split at `|`, `&&`, `||`, `;` and `&`; each command is
//! translated on its own and redirections (`> out.txt`, `2>/dev/null`) are
//! carried across.

use crate::dialect::Dialect::{self, Cmd, Posix, PowerShell};
use crate::emit::emit;
use crate::parse::parse;
use crate::tokenize::{split_ops, split_redirect, tokenize, Piece};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Translation {
    /// Already in the target dialect; run the line unchanged.
    Native,
    /// Rewritten from another dialect.
    Translated { from: Dialect, command: String },
    /// No rule applies; run the line unchanged. Carries a reason worth
    /// showing the user, if there is one.
    Untranslated(Option<&'static str>),
}

enum Simple {
    Native,
    Translated(Dialect, String),
    Unknown,
}

fn translate_simple(cmd: &str, target: Dialect) -> Simple {
    let Some(tokens) = tokenize(cmd) else {
        return Simple::Unknown;
    };
    match parse(&tokens, target) {
        None => Simple::Unknown,
        Some((from, _)) if from == target => Simple::Native,
        Some((from, intent)) => Simple::Translated(from, emit(&intent, from, target)),
    }
}

/// `/dev/null` ↔ `NUL` ↔ `$null` in a redirection like `2>/dev/null`.
fn convert_redirect(redir: &str, target: Dialect) -> String {
    let null = match target {
        Posix => "/dev/null",
        Cmd => "NUL",
        PowerShell => "$null",
    };
    let mut out = String::new();
    let mut rest = redir;
    while let Some(i) = rest.find(['>', '<']) {
        out.push_str(&rest[..=i]);
        rest = &rest[i + 1..];
        // Keep `>>`, `>&1` etc. attached to the operator.
        let op_len = rest
            .find(|c: char| !matches!(c, '>' | '&' | '0'..='9'))
            .unwrap_or(rest.len());
        out.push_str(&rest[..op_len]);
        rest = &rest[op_len..];
        let ws = rest.len() - rest.trim_start().len();
        out.push_str(&rest[..ws]);
        rest = &rest[ws..];
        let word_len = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '<')
            .unwrap_or(rest.len());
        let word = &rest[..word_len];
        let is_null = word == "/dev/null" || word.eq_ignore_ascii_case("nul") || word == "$null";
        out.push_str(if is_null { null } else { word });
        rest = &rest[word_len..];
    }
    out.push_str(rest);
    out
}

/// The dialect a redirection was written in, judging by its null device.
fn redirect_dialect(redir: &str) -> Option<Dialect> {
    if redir.contains("/dev/null") {
        Some(Posix)
    } else if redir.contains("$null") {
        Some(PowerShell)
    } else if redir.to_ascii_lowercase().contains("nul") {
        Some(Cmd)
    } else {
        None
    }
}

pub fn translate(line: &str, target: Dialect) -> Translation {
    let Some(pieces) = split_ops(line) else {
        return Translation::Untranslated(Some(
            "command substitution and unbalanced quotes are passed through as-is",
        ));
    };
    if pieces.is_empty() {
        return Translation::Native;
    }
    let chained = pieces.len() > 1;

    let mut out: Vec<String> = Vec::new();
    let mut from: Option<Dialect> = None;
    let mut changed = false;
    let mut any_unknown = false;

    for piece in &pieces {
        match piece {
            Piece::Op(op) => {
                // cmd has no `;`; its sequential operator is `&`.
                if *op == ";" && target == Cmd {
                    out.push("&".into());
                    changed = true;
                } else {
                    out.push(op.to_string());
                }
            }
            Piece::Command(cmd) => {
                let (body, redir) = split_redirect(cmd);
                let mut text = match translate_simple(body, target) {
                    Simple::Native => body.to_string(),
                    Simple::Unknown => {
                        any_unknown = true;
                        body.to_string()
                    }
                    Simple::Translated(f, command) => {
                        from.get_or_insert(f);
                        changed = true;
                        // Multi-command translations (`if exist … else …`)
                        // must be grouped so the chain binds correctly.
                        if chained && (command.contains(" & ") || command.starts_with("if ")) {
                            format!("({command})")
                        } else {
                            command
                        }
                    }
                };
                if !redir.is_empty() {
                    let new_redir = convert_redirect(redir, target);
                    if new_redir != redir {
                        changed = true;
                        if let Some(d) = redirect_dialect(redir) {
                            from.get_or_insert(d);
                        }
                    }
                    text.push(' ');
                    text.push_str(&new_redir);
                }
                out.push(text);
            }
        }
    }

    if changed {
        let from = from.unwrap_or(if target == Posix { Cmd } else { Posix });
        // cmd's `&` means "then"; in POSIX it would mean "in the background".
        if target == Posix && from == Cmd {
            for item in out.iter_mut().filter(|s| *s == "&") {
                *item = ";".into();
            }
        }
        let mut command = String::new();
        for item in &out {
            if item != ";" && !command.is_empty() {
                command.push(' ');
            }
            command.push_str(item);
        }
        Translation::Translated { from, command }
    } else if any_unknown && !chained {
        Translation::Untranslated(None)
    } else {
        Translation::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(line: &str, target: Dialect) -> String {
        match translate(line, target) {
            Translation::Translated { command, .. } => command,
            other => panic!("expected a translation for {line:?}, got {other:?}"),
        }
    }

    // ---- POSIX → Windows ---------------------------------------------------

    #[test]
    fn ls_to_dir() {
        assert_eq!(tr("ls", Cmd), "dir /B");
        assert_eq!(tr("ls -la", Cmd), "dir /A");
        assert_eq!(tr("ls -R src/main", Cmd), r"dir /S /B src\main");
    }

    #[test]
    fn rm_rf_to_cmd() {
        let out = tr("rm -rf build", Cmd);
        assert!(out.contains(r#"rmdir /S /Q "build""#), "{out}");
        assert!(out.contains(r#"del /F /Q "build""#), "{out}");
    }

    #[test]
    fn cp_r_quotes_spaces() {
        assert_eq!(
            tr(r#"cp -r src "my dest""#, Cmd),
            r#"xcopy src "my dest" /E /I /Y"#
        );
    }

    #[test]
    fn misc_posix_to_cmd() {
        assert_eq!(tr("cat notes.txt", Cmd), "type notes.txt");
        assert_eq!(tr("mkdir -p a/b", Cmd), r"mkdir a\b");
        assert_eq!(tr("kill -9 1234", Cmd), "taskkill /F /PID 1234");
        assert_eq!(tr("pkill notepad", Cmd), "taskkill /IM notepad.exe");
        assert_eq!(
            tr("grep -in todo main.rs", Cmd),
            r#"findstr /I /N "todo" main.rs"#
        );
        assert_eq!(tr("grep -r todo", Cmd), r#"findstr /S "todo" *"#);
        assert_eq!(tr("which cargo", Cmd), "where cargo");
        assert_eq!(tr("clear", Cmd), "cls");
        assert_eq!(tr("ps aux", Cmd), "tasklist");
        assert_eq!(tr("ifconfig", Cmd), "ipconfig /all");
        assert_eq!(tr("touch a.txt", Cmd), r#"type nul >> "a.txt""#);
        assert_eq!(tr("env", Cmd), "set");
        assert_eq!(tr("sleep 5", Cmd), "timeout /t 5 /nobreak >nul");
        assert_eq!(tr("ping -c 3 example.com", Cmd), "ping -n 3 example.com");
        assert_eq!(tr("find . -name '*.rs'", Cmd), "dir /S /B *.rs");
        assert_eq!(tr("find src -name '*.rs'", Cmd), r"dir /S /B src\*.rs");
    }

    // ---- Windows → POSIX ---------------------------------------------------

    #[test]
    fn dir_to_ls() {
        assert_eq!(tr("dir", Posix), "ls -l");
        assert_eq!(tr("dir /s /b", Posix), "ls -R");
        assert_eq!(tr("DIR /A src\\main", Posix), "ls -al src/main");
        assert_eq!(tr("dir /s /b *.txt", Posix), "find . -iname '*.txt'");
    }

    #[test]
    fn misc_cmd_to_posix() {
        assert_eq!(tr("del /q a.txt", Posix), "rm -f a.txt");
        assert_eq!(tr("copy a.txt b.txt", Posix), "cp a.txt b.txt");
        assert_eq!(tr("xcopy src dst /E /I", Posix), "cp -r src dst");
        assert_eq!(tr("move a b", Posix), "mv a b");
        assert_eq!(tr("rd /s /q build", Posix), "rm -r build");
        assert_eq!(tr("type notes.txt", Posix), "cat notes.txt");
        assert_eq!(tr("cls", Posix), "clear");
        assert_eq!(tr("taskkill /F /IM notepad.exe", Posix), "pkill -9 notepad");
        assert_eq!(tr("taskkill /PID 42", Posix), "kill 42");
        assert_eq!(tr("findstr /s /i hello", Posix), "grep -ir 'hello'");
        assert_eq!(tr("where cargo", Posix), "which cargo");
        assert_eq!(tr("tasklist", Posix), "ps aux");
        assert_eq!(tr("set", Posix), "env");
        assert_eq!(tr("timeout /t 3 /nobreak", Posix), "sleep 3");
        assert_eq!(tr("ping -n 2 example.com", Posix), "ping -c 2 example.com");
        assert_eq!(tr("ping -t example.com", Posix), "ping example.com");
    }

    // ---- PowerShell ---------------------------------------------------------

    #[test]
    fn powershell_to_cmd_and_posix() {
        assert_eq!(tr("Get-ChildItem -Force", Posix), "ls -al");
        assert_eq!(tr("gci -Recurse -Name", Cmd), "dir /S /B");
        assert_eq!(tr("Remove-Item -Recurse -Force dist", Posix), "rm -rf dist");
        assert_eq!(tr("Copy-Item a.txt b.txt", Posix), "cp a.txt b.txt");
        assert_eq!(
            tr("New-Item -ItemType Directory logs", Posix),
            "mkdir -p logs"
        );
        assert_eq!(tr("ni notes.md", Posix), "touch notes.md");
        assert_eq!(tr("Get-Content README.md", Posix), "cat README.md");
        assert_eq!(tr("Stop-Process -Name node -Force", Posix), "pkill -9 node");
        assert_eq!(
            tr("Stop-Process -Id 12,34", Cmd),
            "taskkill /PID 12 /PID 34"
        );
        assert_eq!(
            tr("Select-String -Pattern todo -Path main.rs", Posix),
            "grep -in 'todo' main.rs"
        );
        assert_eq!(tr("gci env:", Posix), "env");
        assert_eq!(tr("Start-Sleep 2", Posix), "sleep 2");
        assert_eq!(
            tr("Test-Connection example.com -Count 2", Posix),
            "ping -c 2 example.com"
        );
        // Abbreviated, case-insensitive parameters, like PowerShell itself.
        assert_eq!(tr("remove-item -rec -fo x", Posix), "rm -rf x");
    }

    #[test]
    fn to_powershell() {
        assert_eq!(tr("ls -la", PowerShell), "Get-ChildItem -Force");
        assert_eq!(
            tr("rm -rf a b", PowerShell),
            "Remove-Item -Recurse -Force a, b"
        );
        assert_eq!(tr("del /q a.txt", PowerShell), "Remove-Item -Force a.txt");
        assert_eq!(tr("kill -9 42", PowerShell), "Stop-Process -Id 42 -Force");
        assert_eq!(
            tr("grep -ri todo src", PowerShell),
            "Get-ChildItem -Recurse -File src | Select-String -Pattern 'todo'"
        );
        assert_eq!(
            tr("cat $HOME/.bashrc", PowerShell),
            r"Get-Content $HOME\.bashrc"
        );
    }

    #[test]
    fn unknown_powershell_params_are_not_guessed() {
        assert_eq!(
            translate("Remove-Item -WhatIf x", Posix),
            Translation::Untranslated(None)
        );
        assert_eq!(
            translate("New-Item -Force a.txt", Posix),
            Translation::Untranslated(None)
        );
    }

    // ---- variables -----------------------------------------------------------

    #[test]
    fn variables_follow_the_dialect() {
        assert_eq!(tr("echo $HOME", Cmd), "echo %USERPROFILE%");
        assert_eq!(tr("echo %USERNAME%", Posix), "echo $USER");
        assert_eq!(tr("echo $env:USERNAME", Posix), "echo $USER");
        assert_eq!(tr("cat ~/notes.txt", Cmd), r"type %USERPROFILE%\notes.txt");
        assert_eq!(
            tr(r"type %USERPROFILE%\notes.txt", Posix),
            "cat $HOME/notes.txt"
        );
        assert_eq!(tr(r#"echo "hello $USER""#, Cmd), "echo hello %USERNAME%");
    }

    // ---- pipes, chains, redirection ------------------------------------------

    #[test]
    fn pipelines_translate_each_command() {
        assert_eq!(tr("ls -la | grep foo", Cmd), r#"dir /A | findstr "foo""#);
        assert_eq!(
            tr("tasklist | findstr /i node", Posix),
            "ps aux | grep -i 'node'"
        );
        assert_eq!(
            tr("dir /b | findstr /i readme", PowerShell),
            "Get-ChildItem -Name | Select-String -Pattern 'readme'"
        );
    }

    #[test]
    fn chains_translate_each_command() {
        assert_eq!(
            tr("mkdir -p out && cp a.txt out", Cmd),
            r"mkdir out && copy /Y a.txt out"
        );
        assert_eq!(tr("cls & dir", Posix), "clear; ls -l");
        assert_eq!(tr("clear; ls", Cmd), "cls & dir /B");
        assert_eq!(
            tr("cd ~/app && npm install", Cmd),
            r"cd /d %USERPROFILE%\app && npm install"
        );
        // Multi-command translations get grouped.
        let out = tr("rm -rf build && cargo build", Cmd);
        assert!(out.starts_with("(if exist"), "{out}");
        assert!(out.ends_with(") && cargo build"), "{out}");
    }

    #[test]
    fn redirection_is_carried_across() {
        assert_eq!(tr("ls > files.txt", Cmd), "dir /B > files.txt");
        assert_eq!(tr("cargo build 2>/dev/null", Cmd), "cargo build 2>NUL");
        assert_eq!(tr("dir 2>nul", Posix), "ls -l 2>/dev/null");
        assert_eq!(tr("make >nul 2>&1", Posix), "make >/dev/null 2>&1");
        assert_eq!(
            tr("cargo build 2>/dev/null", PowerShell),
            "cargo build 2>$null"
        );
    }

    // ---- safety ------------------------------------------------------------

    #[test]
    fn native_commands_untouched() {
        assert_eq!(translate("ls -la", Posix), Translation::Native);
        assert_eq!(translate("dir /s", Cmd), Translation::Native);
        assert_eq!(translate("mkdir foo", Cmd), Translation::Native);
        assert_eq!(translate("mkdir foo", Posix), Translation::Native);
        assert_eq!(translate("ls -la | grep x", Posix), Translation::Native);
        assert_eq!(translate("echo hello", Cmd), Translation::Native);
        assert_eq!(translate("ping example.com", Cmd), Translation::Native);
        assert_eq!(
            translate("cargo build && cargo test", Cmd),
            Translation::Native
        );
    }

    #[test]
    fn unknown_flags_are_not_guessed() {
        assert_eq!(translate("rm -i x", Cmd), Translation::Untranslated(None));
        assert_eq!(
            translate("dir /O:D", Posix),
            Translation::Untranslated(None)
        );
        assert_eq!(
            translate("cargo build", Cmd),
            Translation::Untranslated(None)
        );
        assert_eq!(
            translate("find . -type f", Cmd),
            Translation::Untranslated(None)
        );
    }

    #[test]
    fn command_substitution_passes_through() {
        assert!(matches!(
            translate("echo $(whoami)", Cmd),
            Translation::Untranslated(Some(_))
        ));
    }
}
