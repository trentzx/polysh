//! Command line (in any dialect) → [`Intent`].
//!
//! Each parser only accepts flags it fully understands. Anything else returns
//! `None`, so the line runs unchanged instead of being silently mistranslated.

use crate::dialect::Dialect::{self, Cmd, Posix};
use crate::intent::Intent;

/// How `/X` style switches are recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slash {
    /// Never: `/foo` is a path.
    Off,
    /// Only single letters (`/S`), for commands shared by both dialects,
    /// where `/usr` could be a real path.
    Strict,
    /// Any short switch (`/A:H`, `/PID`), for cmd-only commands.
    Loose,
}

#[derive(Debug, Default)]
struct Args {
    /// POSIX short flags, e.g. `-la` → `['l', 'a']`.
    short: Vec<char>,
    /// POSIX long flags without the `--`.
    long: Vec<String>,
    /// cmd switches, upper-cased, without the `/`.
    slash: Vec<String>,
    /// Everything else.
    pos: Vec<String>,
}

impl Args {
    fn split(tokens: &[String], dash: bool, slash: Slash) -> Args {
        let mut a = Args::default();
        let mut end_of_flags = false;
        for t in tokens {
            if !end_of_flags && dash {
                if t == "--" {
                    end_of_flags = true;
                    continue;
                }
                if let Some(name) = t.strip_prefix("--") {
                    a.long.push(name.to_string());
                    continue;
                }
                if t.len() > 1 && t.starts_with('-') {
                    a.short.extend(t[1..].chars());
                    continue;
                }
            }
            if !end_of_flags && is_slash_flag(t, slash) {
                a.slash.push(t[1..].to_ascii_uppercase());
                continue;
            }
            a.pos.push(t.clone());
        }
        a
    }

    fn has(&self, c: char) -> bool {
        self.short.contains(&c)
    }

    fn any(&self, cs: &str) -> bool {
        cs.chars().any(|c| self.has(c))
    }

    fn long(&self, name: &str) -> bool {
        self.long.iter().any(|l| l == name)
    }

    /// Is the cmd switch `/name` present (ignoring `:value` suffixes)?
    fn sw(&self, name: &str) -> bool {
        self.slash.iter().any(|s| switch_base(s) == name)
    }

    /// True if every flag given is in the allowed sets.
    fn only(&self, short: &str, long: &[&str], slash: &[&str]) -> bool {
        self.short.iter().all(|c| short.contains(*c))
            && self.long.iter().all(|l| long.contains(&l.as_str()))
            && self.slash.iter().all(|s| slash.contains(&switch_base(s)))
    }
}

/// `A:H` → `A`, `PID` → `PID`.
fn switch_base(s: &str) -> &str {
    let end = s
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(s.len());
    &s[..end]
}

fn is_slash_flag(t: &str, mode: Slash) -> bool {
    let Some(rest) = t.strip_prefix('/') else {
        return false;
    };
    match mode {
        Slash::Off => false,
        Slash::Strict => rest.len() == 1 && rest.chars().all(|c| c.is_ascii_alphabetic()),
        Slash::Loose => {
            !rest.is_empty()
                && rest.len() <= 8
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '-')
        }
    }
}

fn posix_args(r: &[String]) -> Args {
    Args::split(r, true, Slash::Off)
}

fn cmd_args(r: &[String]) -> Args {
    Args::split(r, false, Slash::Loose)
}

fn two(mut v: Vec<String>) -> Option<(String, String)> {
    if v.len() != 2 {
        return None;
    }
    let dst = v.pop()?;
    let src = v.pop()?;
    Some((src, dst))
}

fn non_empty(v: Vec<String>) -> Option<Vec<String>> {
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

fn is_kill_force(t: &str) -> bool {
    t == "-9" || t.eq_ignore_ascii_case("-KILL") || t.eq_ignore_ascii_case("-SIGKILL")
}

/// Parse a tokenized command line. `native` is used as the source dialect
/// for commands that are spelled the same in both (e.g. plain `mkdir foo`).
pub fn parse(tokens: &[String], native: Dialect) -> Option<(Dialect, Intent)> {
    let (first, rest) = tokens.split_first()?;
    let lower = first.to_ascii_lowercase();
    let name = lower.strip_suffix(".exe").unwrap_or(&lower);

    match name {
        // ---- list ----------------------------------------------------------
        "ls" => {
            let a = posix_args(rest);
            if !a.only("aAlRh1", &["all"], &[]) {
                return None;
            }
            Some((
                Posix,
                Intent::ListDir {
                    all: a.any("aA") || a.long("all"),
                    long: a.has('l'),
                    recursive: a.has('R'),
                    paths: a.pos,
                },
            ))
        }
        "dir" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &["A", "S", "B", "W", "P"]) {
                return None;
            }
            Some((
                Cmd,
                Intent::ListDir {
                    all: a.sw("A"),
                    long: !(a.sw("B") || a.sw("W")),
                    recursive: a.sw("S"),
                    paths: a.pos,
                },
            ))
        }

        // ---- copy / move ---------------------------------------------------
        "cp" => {
            let a = posix_args(rest);
            if !a.only("rRafv", &["recursive"], &[]) {
                return None;
            }
            let recursive = a.any("rRa") || a.long("recursive");
            let (src, dst) = two(a.pos)?;
            Some((
                Posix,
                Intent::Copy {
                    recursive,
                    src,
                    dst,
                },
            ))
        }
        "copy" | "xcopy" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &["Y", "V", "E", "S", "I", "Q", "H"]) {
                return None;
            }
            let recursive = name == "xcopy" && (a.sw("E") || a.sw("S"));
            let mut pos = a.pos;
            if pos.len() == 1 {
                pos.push(".".to_string());
            }
            let (src, dst) = two(pos)?;
            Some((
                Cmd,
                Intent::Copy {
                    recursive,
                    src,
                    dst,
                },
            ))
        }
        "mv" => {
            let a = posix_args(rest);
            if !a.only("fv", &[], &[]) {
                return None;
            }
            let (src, dst) = two(a.pos)?;
            Some((Posix, Intent::Move { src, dst }))
        }
        "move" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &["Y"]) {
                return None;
            }
            let (src, dst) = two(a.pos)?;
            Some((Cmd, Intent::Move { src, dst }))
        }

        // ---- delete --------------------------------------------------------
        "rm" => {
            let a = posix_args(rest);
            if !a.only("rRfv", &["recursive", "force"], &[]) {
                return None;
            }
            Some((
                Posix,
                Intent::Remove {
                    recursive: a.any("rR") || a.long("recursive"),
                    force: a.has('f') || a.long("force"),
                    paths: non_empty(a.pos)?,
                },
            ))
        }
        "del" | "erase" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &["F", "Q"]) {
                return None;
            }
            Some((
                Cmd,
                Intent::Remove {
                    recursive: false,
                    force: a.sw("F") || a.sw("Q"),
                    paths: non_empty(a.pos)?,
                },
            ))
        }

        // ---- folders -------------------------------------------------------
        "rd" | "rmdir" => {
            let a = Args::split(rest, true, Slash::Strict);
            let source = if name == "rd" || !a.slash.is_empty() {
                Cmd
            } else if !a.short.is_empty() {
                Posix
            } else {
                native
            };
            if !a.only("", &[], &["S", "Q"]) {
                return None;
            }
            Some((
                source,
                Intent::RemoveDir {
                    recursive: a.sw("S"),
                    paths: non_empty(a.pos)?,
                },
            ))
        }
        "md" | "mkdir" => {
            let a = Args::split(rest, true, Slash::Strict);
            if !a.only("pv", &["parents"], &[]) {
                return None;
            }
            let source = if name == "md" {
                Cmd
            } else if a.has('p') || a.long("parents") {
                Posix
            } else {
                native
            };
            Some((
                source,
                Intent::MakeDir {
                    paths: non_empty(a.pos)?,
                },
            ))
        }

        // ---- files & screen ------------------------------------------------
        "cat" => {
            let a = posix_args(rest);
            if !a.only("", &[], &[]) {
                return None;
            }
            Some((
                Posix,
                Intent::PrintFile {
                    paths: non_empty(a.pos)?,
                },
            ))
        }
        "type" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &[]) {
                return None;
            }
            Some((
                Cmd,
                Intent::PrintFile {
                    paths: non_empty(a.pos)?,
                },
            ))
        }
        "clear" if rest.is_empty() => Some((Posix, Intent::Clear)),
        "cls" if rest.is_empty() => Some((Cmd, Intent::Clear)),

        // ---- network -------------------------------------------------------
        "ifconfig" if rest.is_empty() || rest == ["-a"] => {
            Some((Posix, Intent::NetConfig { all: true }))
        }
        "ip" if rest.len() == 1 && matches!(rest[0].as_str(), "a" | "addr" | "address") => {
            Some((Posix, Intent::NetConfig { all: true }))
        }
        "ipconfig" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &["ALL"]) || !a.pos.is_empty() {
                return None;
            }
            Some((Cmd, Intent::NetConfig { all: a.sw("ALL") }))
        }

        // ---- processes -----------------------------------------------------
        "ps" if rest.is_empty() || rest == ["aux"] || rest == ["-ef"] || rest == ["-e"] => {
            Some((Posix, Intent::ProcessList))
        }
        "tasklist" if rest.is_empty() => Some((Cmd, Intent::ProcessList)),
        "kill" => {
            let force = rest.iter().any(|t| is_kill_force(t));
            if rest.iter().any(|t| t.starts_with('-') && !is_kill_force(t)) {
                return None;
            }
            let pids: Vec<String> = rest
                .iter()
                .filter(|t| !t.starts_with('-'))
                .cloned()
                .collect();
            if !pids.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
                return None;
            }
            Some((
                Posix,
                Intent::KillPid {
                    force,
                    pids: non_empty(pids)?,
                },
            ))
        }
        "pkill" | "killall" => {
            let force = rest.iter().any(|t| is_kill_force(t));
            if rest.iter().any(|t| t.starts_with('-') && !is_kill_force(t)) {
                return None;
            }
            let names: Vec<&String> = rest.iter().filter(|t| !t.starts_with('-')).collect();
            if names.len() != 1 {
                return None;
            }
            Some((
                Posix,
                Intent::KillName {
                    force,
                    name: names[0].clone(),
                },
            ))
        }
        "taskkill" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &["F", "PID", "IM", "T"]) {
                return None;
            }
            let force = a.sw("F");
            if a.sw("IM") {
                let (name,) = match a.pos.as_slice() {
                    [n] => (n.clone(),),
                    _ => return None,
                };
                Some((Cmd, Intent::KillName { force, name }))
            } else if a.sw("PID") {
                Some((
                    Cmd,
                    Intent::KillPid {
                        force,
                        pids: non_empty(a.pos)?,
                    },
                ))
            } else {
                None
            }
        }

        // ---- lookup & search ----------------------------------------------
        "which" => {
            let a = posix_args(rest);
            if !a.only("a", &[], &[]) {
                return None;
            }
            Some((
                Posix,
                Intent::Which {
                    names: non_empty(a.pos)?,
                },
            ))
        }
        "where" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &[]) {
                return None;
            }
            Some((
                Cmd,
                Intent::Which {
                    names: non_empty(a.pos)?,
                },
            ))
        }
        "grep" => {
            let a = posix_args(rest);
            if !a.only("irRn", &["ignore-case", "recursive", "line-number"], &[]) {
                return None;
            }
            let ignore_case = a.has('i') || a.long("ignore-case");
            let recursive = a.any("rR") || a.long("recursive");
            let line_numbers = a.has('n') || a.long("line-number");
            let mut pos = a.pos.into_iter();
            let pattern = pos.next()?;
            Some((
                Posix,
                Intent::Search {
                    ignore_case,
                    recursive,
                    line_numbers,
                    pattern,
                    paths: pos.collect(),
                },
            ))
        }
        "findstr" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &["I", "S", "N"]) {
                return None;
            }
            let ignore_case = a.sw("I");
            let recursive = a.sw("S");
            let line_numbers = a.sw("N");
            let mut pos = a.pos.into_iter();
            let pattern = pos.next()?;
            Some((
                Cmd,
                Intent::Search {
                    ignore_case,
                    recursive,
                    line_numbers,
                    pattern,
                    paths: pos.collect(),
                },
            ))
        }

        _ => None,
    }
}
