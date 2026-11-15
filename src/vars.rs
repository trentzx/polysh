//! Environment variables across dialects: `$HOME` ↔ `%USERPROFILE%` ↔ `$HOME`,
//! `$USER` ↔ `%USERNAME%` ↔ `$env:USERNAME`, and so on.

use crate::dialect::Dialect;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Lit(String),
    /// Canonical name: HOME, USER, TEMP, PWD, or the original name.
    Var(String),
}

/// True if `s` references an environment variable in `from`'s syntax.
pub fn has_vars(s: &str, from: Dialect) -> bool {
    split(s, from).iter().any(|p| matches!(p, Piece::Var(_)))
}

/// Rewrite variable references from `from`'s syntax into `to`'s.
pub fn convert(s: &str, from: Dialect, to: Dialect) -> String {
    if from == to {
        return s.to_string();
    }
    let pieces = split(s, from);
    let mut out = String::new();
    for (i, p) in pieces.iter().enumerate() {
        match p {
            Piece::Lit(t) => out.push_str(t),
            Piece::Var(name) => {
                let next_is_word = matches!(
                    pieces.get(i + 1),
                    Some(Piece::Lit(t)) if t.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                );
                out.push_str(&render(name, to, next_is_word));
            }
        }
    }
    out
}

/// Expand references in any dialect's syntax to their actual values
/// (used by the `cd` built-in).
pub fn expand(s: &str) -> String {
    let mut cur = s.to_string();
    for d in [Dialect::PowerShell, Dialect::Cmd, Dialect::Posix] {
        cur = split(&cur, d)
            .into_iter()
            .map(|p| match p {
                Piece::Lit(t) => t,
                Piece::Var(name) => lookup(&name).unwrap_or_default(),
            })
            .collect();
    }
    cur
}

fn lookup(canon: &str) -> Option<String> {
    let env = |k: &str| std::env::var(k).ok();
    match canon {
        "HOME" => env("HOME").or_else(|| env("USERPROFILE")),
        "USER" => env("USER").or_else(|| env("USERNAME")),
        "TEMP" => env("TEMP")
            .or_else(|| env("TMPDIR"))
            .or_else(|| Some("/tmp".into())),
        "PWD" => std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string()),
        other => env(other),
    }
}

fn canonical(name: &str, from: Dialect) -> String {
    // POSIX names are case-sensitive; cmd and PowerShell aren't.
    let key = match from {
        Dialect::Posix => name.to_string(),
        _ => name.to_ascii_uppercase(),
    };
    match key.as_str() {
        "HOME" | "USERPROFILE" => "HOME".into(),
        "USER" | "USERNAME" => "USER".into(),
        "TEMP" | "TMP" | "TMPDIR" => "TEMP".into(),
        "PWD" | "CD" => "PWD".into(),
        _ => name.to_string(),
    }
}

fn render(canon: &str, to: Dialect, next_is_word: bool) -> String {
    match to {
        Dialect::Posix => {
            let name = match canon {
                "TEMP" => return "${TMPDIR:-/tmp}".into(),
                n => n,
            };
            if next_is_word {
                format!("${{{name}}}")
            } else {
                format!("${name}")
            }
        }
        Dialect::Cmd => {
            let name = match canon {
                "HOME" => "USERPROFILE",
                "USER" => "USERNAME",
                "PWD" => "CD",
                n => n,
            };
            format!("%{name}%")
        }
        Dialect::PowerShell => {
            let (prefix, name) = match canon {
                "HOME" => ("", "HOME"),
                "PWD" => ("", "PWD"),
                "USER" => ("env:", "USERNAME"),
                n => ("env:", n),
            };
            if next_is_word {
                format!("${{{prefix}{name}}}")
            } else {
                format!("${prefix}{name}")
            }
        }
    }
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Split `s` into literal text and variable references in `from`'s syntax.
fn split(s: &str, from: Dialect) -> Vec<Piece> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut i = 0;

    let push_var = |out: &mut Vec<Piece>, lit: &mut String, name: String| {
        if !lit.is_empty() {
            out.push(Piece::Lit(std::mem::take(lit)));
        }
        out.push(Piece::Var(name));
    };

    // Leading `~` means home in POSIX and PowerShell.
    if matches!(from, Dialect::Posix | Dialect::PowerShell)
        && chars.first() == Some(&'~')
        && matches!(chars.get(1), None | Some('/') | Some('\\'))
    {
        push_var(&mut out, &mut lit, "HOME".into());
        i = 1;
    }

    while i < chars.len() {
        let c = chars[i];
        match from {
            Dialect::Posix if c == '$' => {
                if chars.get(i + 1) == Some(&'{') {
                    if let Some(end) = chars[i + 2..].iter().position(|&ch| ch == '}') {
                        let name: String = chars[i + 2..i + 2 + end].iter().collect();
                        if !name.is_empty() && name.chars().all(is_name_char) {
                            push_var(&mut out, &mut lit, canonical(&name, from));
                            i += end + 3;
                            continue;
                        }
                    }
                } else if chars.get(i + 1).is_some_and(|&ch| is_name_start(ch)) {
                    let len = chars[i + 1..]
                        .iter()
                        .take_while(|&&ch| is_name_char(ch))
                        .count();
                    let name: String = chars[i + 1..i + 1 + len].iter().collect();
                    push_var(&mut out, &mut lit, canonical(&name, from));
                    i += len + 1;
                    continue;
                }
            }
            Dialect::Cmd if c == '%' => {
                if let Some(end) = chars[i + 1..].iter().position(|&ch| ch == '%') {
                    let name: String = chars[i + 1..i + 1 + end].iter().collect();
                    if name.starts_with(is_name_start) && name.chars().all(is_name_char) {
                        push_var(&mut out, &mut lit, canonical(&name, from));
                        i += end + 2;
                        continue;
                    }
                }
            }
            Dialect::PowerShell if c == '$' => {
                let rest: String = chars[i + 1..].iter().collect();
                if rest
                    .get(..4)
                    .is_some_and(|p| p.eq_ignore_ascii_case("env:"))
                {
                    let len = rest[4..].chars().take_while(|&ch| is_name_char(ch)).count();
                    if len > 0 {
                        push_var(&mut out, &mut lit, canonical(&rest[4..4 + len], from));
                        i += 5 + len;
                        continue;
                    }
                }
                let len = rest.chars().take_while(|&ch| is_name_char(ch)).count();
                let name = &rest[..len];
                if name.eq_ignore_ascii_case("HOME") || name.eq_ignore_ascii_case("PWD") {
                    push_var(&mut out, &mut lit, canonical(name, from));
                    i += 1 + len;
                    continue;
                }
            }
            _ => {}
        }
        lit.push(c);
        i += 1;
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::Dialect::{Cmd, Posix, PowerShell};

    #[test]
    fn posix_to_cmd() {
        assert_eq!(convert("$HOME/x", Posix, Cmd), "%USERPROFILE%/x");
        assert_eq!(convert("~/notes", Posix, Cmd), "%USERPROFILE%/notes");
        assert_eq!(convert("${USER}_bak", Posix, Cmd), "%USERNAME%_bak");
        assert_eq!(convert("$MY_VAR", Posix, Cmd), "%MY_VAR%");
        assert_eq!(convert("100$", Posix, Cmd), "100$");
    }

    #[test]
    fn cmd_to_posix_and_powershell() {
        assert_eq!(convert(r"%USERPROFILE%\x", Cmd, Posix), r"$HOME\x");
        assert_eq!(convert("%TEMP%", Cmd, Posix), "${TMPDIR:-/tmp}");
        assert_eq!(convert("%USERNAME%", Cmd, PowerShell), "$env:USERNAME");
        assert_eq!(convert("100%", Cmd, Posix), "100%");
        assert_eq!(convert("%PATH%x", Cmd, Posix), "${PATH}x");
    }

    #[test]
    fn powershell_to_others() {
        assert_eq!(convert("$env:USERNAME", PowerShell, Posix), "$USER");
        assert_eq!(convert("$HOME", PowerShell, Cmd), "%USERPROFILE%");
        assert_eq!(convert("$x", PowerShell, Cmd), "$x");
    }
}
