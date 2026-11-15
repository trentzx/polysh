//! Glue: a raw command line in, a [`Translation`] out.

use crate::dialect::Dialect;
use crate::emit::emit;
use crate::parse::parse;
use crate::tokenize::{has_operators, tokenize};

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

pub fn translate(line: &str, target: Dialect) -> Translation {
    if has_operators(line) {
        return Translation::Untranslated(Some(
            "pipes, redirection and chaining are passed through as-is",
        ));
    }
    let Some(tokens) = tokenize(line) else {
        return Translation::Untranslated(Some("unbalanced quotes"));
    };
    if tokens.is_empty() {
        return Translation::Native;
    }
    match parse(&tokens, target) {
        None => Translation::Untranslated(None),
        Some((from, _)) if from == target => Translation::Native,
        Some((from, intent)) => Translation::Translated {
            from,
            command: emit(&intent, target),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::Dialect::{Cmd, Posix};

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
    }

    // ---- Windows → POSIX ---------------------------------------------------

    #[test]
    fn dir_to_ls() {
        assert_eq!(tr("dir", Posix), "ls -l");
        assert_eq!(tr("dir /s /b", Posix), "ls -R");
        assert_eq!(tr("DIR /A src\\main", Posix), "ls -al src/main");
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
    }

    // ---- safety ------------------------------------------------------------

    #[test]
    fn native_commands_untouched() {
        assert_eq!(translate("ls -la", Posix), Translation::Native);
        assert_eq!(translate("dir /s", Cmd), Translation::Native);
        assert_eq!(translate("mkdir foo", Cmd), Translation::Native);
        assert_eq!(translate("mkdir foo", Posix), Translation::Native);
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
    }

    #[test]
    fn pipes_pass_through() {
        assert!(matches!(
            translate("ls | grep x", Cmd),
            Translation::Untranslated(Some(_))
        ));
    }
}
