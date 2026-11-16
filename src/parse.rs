//! Command line (in any dialect) → [`Intent`].
//!
//! Each parser only accepts flags it fully understands. Anything else returns
//! `None`, so the line runs unchanged instead of being silently mistranslated.

use crate::dialect::Dialect::{self, Cmd, Posix, PowerShell};
use crate::intent::Intent;
use crate::vars::has_vars;

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

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn has_glob(s: &str) -> bool {
    s.contains(['*', '?'])
}

/// `src\*.txt` → (`src`, `*.txt`); `*.txt` → (`.`, `*.txt`).
fn split_parent(p: &str) -> (String, String) {
    match p.rfind(['\\', '/']) {
        Some(i) => (p[..i].to_string(), p[i + 1..].to_string()),
        None => (".".to_string(), p.to_string()),
    }
}

/// `head`/`tail` flags: `-n N`, `-nN`, `-N`, `--lines=N`, and `-f` for tail.
/// At most one file; `tail -f` needs one.
fn parse_head_tail(rest: &[String], tail: bool) -> Option<(String, bool, Option<String>)> {
    let mut lines = "10".to_string();
    let mut follow = false;
    let mut paths = Vec::new();
    let mut it = rest.iter();
    while let Some(t) = it.next() {
        if t == "-f" && tail {
            follow = true;
        } else if t == "-n" || t == "--lines" {
            lines = it.next().filter(|n| is_number(n))?.clone();
        } else if let Some(n) = t.strip_prefix("--lines=").or_else(|| t.strip_prefix("-n")) {
            if !is_number(n) {
                return None;
            }
            lines = n.to_string();
        } else if let Some(n) = t.strip_prefix('-') {
            if !is_number(n) {
                return None;
            }
            lines = n.to_string();
        } else {
            paths.push(t.clone());
        }
    }
    if paths.len() > 1 {
        return None;
    }
    let path = paths.pop();
    if follow && path.is_none() {
        return None;
    }
    Some((lines, follow, path))
}

/// Does `start X` mean "open X" rather than "run program X"?
fn looks_openable(p: &str) -> bool {
    if p.contains("://") {
        return true;
    }
    let lower = p.to_ascii_lowercase();
    let program = [".exe", ".bat", ".cmd", ".com", ".ps1", ".msi"]
        .iter()
        .any(|e| lower.ends_with(e));
    !program && !p.starts_with('/') && p.contains(['.', '\\', '/'])
}

fn single(mut v: Vec<String>) -> Option<String> {
    if v.len() == 1 {
        v.pop()
    } else {
        None
    }
}

/// Parsed PowerShell parameters.
#[derive(Debug, Default)]
struct Ps {
    switches: Vec<&'static str>,
    values: Vec<(&'static str, String)>,
    pos: Vec<String>,
}

impl Ps {
    fn has(&self, name: &str) -> bool {
        self.switches.contains(&name)
    }

    fn get(&self, name: &str) -> Option<String> {
        self.values
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
    }
}

/// Parse PowerShell `-Param [value]` arguments. Names are case-insensitive
/// and may be abbreviated to any unique prefix, like PowerShell itself.
/// `spec` lists `(name, takes_value)`; an unknown or ambiguous parameter
/// returns `None`.
fn ps_args(tokens: &[String], spec: &[(&'static str, bool)]) -> Option<Ps> {
    let mut out = Ps::default();
    let mut it = tokens.iter();
    while let Some(t) = it.next() {
        if let Some(raw) = t.strip_prefix('-') {
            if raw.starts_with(|c: char| c.is_ascii_alphabetic()) {
                let lower = raw.to_ascii_lowercase();
                let exact = spec.iter().find(|(n, _)| *n == lower);
                let prefixed: Vec<_> = spec.iter().filter(|(n, _)| n.starts_with(&lower)).collect();
                let &(name, takes_value) = match (exact, prefixed.as_slice()) {
                    (Some(e), _) => e,
                    (None, [only]) => *only,
                    _ => return None,
                };
                if takes_value {
                    out.values.push((name, it.next()?.clone()));
                } else {
                    out.switches.push(name);
                }
                continue;
            }
        }
        out.pos.push(t.clone());
    }
    Some(out)
}

/// The dialect whose variable syntax appears in `words`, if any.
fn var_dialect(words: &[String]) -> Option<Dialect> {
    if words
        .iter()
        .any(|w| w.to_ascii_lowercase().contains("$env:"))
    {
        Some(PowerShell)
    } else if words.iter().any(|w| has_vars(w, Cmd)) {
        Some(Cmd)
    } else if words.iter().any(|w| has_vars(w, Posix)) {
        Some(Posix)
    } else {
        None
    }
}

/// PowerShell cmdlets and their PowerShell-only aliases.
fn parse_powershell(name: &str, rest: &[String]) -> Option<Intent> {
    match name {
        "get-childitem" | "gci" => {
            let p = ps_args(
                rest,
                &[
                    ("path", true),
                    ("filter", true),
                    ("recurse", false),
                    ("force", false),
                    ("name", false),
                ],
            )?;
            let mut paths = p.pos.clone();
            paths.extend(p.get("path"));
            if paths.len() == 1 && paths[0].trim_end_matches('\\').eq_ignore_ascii_case("env:") {
                return Some(Intent::ListEnv);
            }
            if let Some(filter) = p.get("filter") {
                if p.has("recurse") {
                    if paths.len() > 1 {
                        return None;
                    }
                    return Some(Intent::FindName {
                        root: paths.pop().unwrap_or_else(|| ".".into()),
                        pattern: filter,
                        ignore_case: true,
                    });
                }
                if !paths.is_empty() {
                    return None;
                }
                paths.push(filter);
            }
            Some(Intent::ListDir {
                all: p.has("force"),
                long: !p.has("name"),
                recursive: p.has("recurse"),
                paths,
            })
        }
        "copy-item" | "cpi" | "move-item" | "mi" => {
            let p = ps_args(
                rest,
                &[
                    ("path", true),
                    ("destination", true),
                    ("recurse", false),
                    ("force", false),
                ],
            )?;
            let mut pos = p.pos.clone();
            if let Some(path) = p.get("path") {
                pos.insert(0, path);
            }
            pos.extend(p.get("destination"));
            let (src, dst) = two(pos)?;
            if name.starts_with('m') {
                Some(Intent::Move { src, dst })
            } else {
                Some(Intent::Copy {
                    recursive: p.has("recurse"),
                    src,
                    dst,
                })
            }
        }
        "remove-item" | "ri" => {
            let p = ps_args(
                rest,
                &[("path", true), ("recurse", false), ("force", false)],
            )?;
            let mut paths = p.pos.clone();
            paths.extend(p.get("path"));
            Some(Intent::Remove {
                recursive: p.has("recurse"),
                force: p.has("force"),
                paths: non_empty(paths)?,
            })
        }
        "new-item" | "ni" => {
            let p = ps_args(
                rest,
                &[
                    ("itemtype", true),
                    ("path", true),
                    ("name", true),
                    ("force", false),
                    ("target", true),
                    ("value", true),
                ],
            )?;
            let mut paths = p.pos.clone();
            paths.extend(p.get("path"));
            let link_target = p.get("target").or_else(|| p.get("value"));
            if let Some(n) = p.get("name") {
                if paths.len() > 1 {
                    return None;
                }
                let joined = match paths.pop() {
                    Some(dir) => format!("{dir}\\{n}"),
                    None => n,
                };
                paths.push(joined);
            }
            let paths = non_empty(paths)?;
            match p.get("itemtype").map(|t| t.to_ascii_lowercase()).as_deref() {
                Some("symboliclink") if !p.has("force") => Some(Intent::Symlink {
                    dir: None,
                    target: link_target?,
                    link: single(paths)?,
                }),
                _ if link_target.is_some() => None,
                Some("directory") | Some("dir") => Some(Intent::MakeDir { paths }),
                // `-Force` on a file truncates it, which isn't `touch`.
                Some("file") | None if !p.has("force") => Some(Intent::Touch { paths }),
                _ => None,
            }
        }
        "get-content" | "gc" => {
            let p = ps_args(
                rest,
                &[
                    ("path", true),
                    ("totalcount", true),
                    ("head", true),
                    ("first", true),
                    ("tail", true),
                    ("last", true),
                    ("wait", false),
                ],
            )?;
            let mut paths = p.pos.clone();
            paths.extend(p.get("path"));
            let head = p
                .get("totalcount")
                .or_else(|| p.get("head"))
                .or_else(|| p.get("first"));
            let tail = p.get("tail").or_else(|| p.get("last"));
            match (head, tail) {
                (None, None) if !p.has("wait") => Some(Intent::PrintFile {
                    paths: non_empty(paths)?,
                }),
                (Some(n), None) if is_number(&n) && !p.has("wait") => Some(Intent::Head {
                    lines: n,
                    path: Some(single(paths)?),
                }),
                (None, Some(n)) if is_number(&n) => Some(Intent::Tail {
                    lines: n,
                    follow: p.has("wait"),
                    path: Some(single(paths)?),
                }),
                _ => None,
            }
        }
        "select-object" | "select" => {
            let p = ps_args(rest, &[("first", true), ("last", true)])?;
            if !p.pos.is_empty() {
                return None;
            }
            match (p.get("first"), p.get("last")) {
                (Some(n), None) if is_number(&n) => Some(Intent::Head {
                    lines: n,
                    path: None,
                }),
                (None, Some(n)) if is_number(&n) => Some(Intent::Tail {
                    lines: n,
                    follow: false,
                    path: None,
                }),
                _ => None,
            }
        }
        "measure-object" | "measure" => {
            let p = ps_args(rest, &[("line", false)])?;
            if p.has("line") && p.pos.is_empty() {
                Some(Intent::CountLines { path: None })
            } else {
                None
            }
        }
        "get-psdrive" | "gdr" => {
            let p = ps_args(rest, &[("psprovider", true)])?;
            match p.get("psprovider") {
                Some(v) if v.eq_ignore_ascii_case("filesystem") && p.pos.is_empty() => {
                    Some(Intent::DiskFree)
                }
                _ => None,
            }
        }
        "get-volume" if rest.is_empty() => Some(Intent::DiskFree),
        "invoke-item" | "ii" => {
            let p = ps_args(rest, &[("path", true)])?;
            let mut paths = p.pos.clone();
            paths.extend(p.get("path"));
            Some(Intent::Open {
                path: single(paths)?,
            })
        }
        "clear-host" if rest.is_empty() => Some(Intent::Clear),
        "get-process" | "gps" if rest.is_empty() => Some(Intent::ProcessList),
        "stop-process" | "spps" => {
            let p = ps_args(rest, &[("id", true), ("name", true), ("force", false)])?;
            let force = p.has("force");
            if let Some(n) = p.get("name") {
                if !p.pos.is_empty() || p.get("id").is_some() {
                    return None;
                }
                return Some(Intent::KillName { force, name: n });
            }
            let mut ids: Vec<String> = p.pos.clone();
            ids.extend(p.get("id"));
            let pids: Vec<String> = ids
                .iter()
                .flat_map(|s| s.split(','))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !pids.iter().all(|p| is_number(p)) {
                return None;
            }
            Some(Intent::KillPid {
                force,
                pids: non_empty(pids)?,
            })
        }
        "get-command" | "gcm" => {
            let p = ps_args(rest, &[])?;
            Some(Intent::Which {
                names: non_empty(p.pos)?,
            })
        }
        "select-string" | "sls" => {
            let p = ps_args(
                rest,
                &[("pattern", true), ("path", true), ("casesensitive", false)],
            )?;
            let mut pos = p.pos.clone().into_iter();
            let pattern = match p.get("pattern") {
                Some(pat) => pat,
                None => pos.next()?,
            };
            let mut paths: Vec<String> = pos.collect();
            paths.extend(p.get("path"));
            Some(Intent::Search {
                ignore_case: !p.has("casesensitive"),
                recursive: false,
                line_numbers: true,
                pattern,
                paths,
            })
        }
        "get-netipconfiguration" | "gip" => {
            ps_args(rest, &[("all", false), ("detailed", false)])?;
            Some(Intent::NetConfig { all: true })
        }
        "set-location" | "sl" => {
            let p = ps_args(rest, &[("path", true)])?;
            let mut paths = p.pos.clone();
            paths.extend(p.get("path"));
            match paths.as_slice() {
                [path] => Some(Intent::ChangeDir { path: path.clone() }),
                _ => None,
            }
        }
        "write-output" => Some(Intent::Echo {
            words: rest.to_vec(),
        }),
        "test-connection" => {
            let p = ps_args(
                rest,
                &[
                    ("count", true),
                    ("targetname", true),
                    ("computername", true),
                ],
            )?;
            let mut hosts = p.pos.clone();
            hosts.extend(p.get("targetname"));
            hosts.extend(p.get("computername"));
            let count = p.get("count");
            if count.as_deref().is_some_and(|c| !is_number(c)) {
                return None;
            }
            match hosts.as_slice() {
                [host] => Some(Intent::Ping {
                    count,
                    forever: false,
                    host: host.clone(),
                }),
                _ => None,
            }
        }
        "start-sleep" => {
            let p = ps_args(rest, &[("seconds", true)])?;
            let mut secs = p.pos.clone();
            secs.extend(p.get("seconds"));
            match secs.as_slice() {
                [s] if is_number(s) => Some(Intent::Sleep { secs: s.clone() }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `ping` is spelled the same everywhere but its flags differ:
/// POSIX `-c N` (default: forever) vs. Windows `-n N` / `-t` (default: 4).
fn parse_ping(rest: &[String], native: Dialect) -> Option<(Dialect, Intent)> {
    let mut source = None;
    let mut count = None;
    let mut forever = false;
    let mut hosts = Vec::new();
    let mut it = rest.iter().peekable();
    while let Some(t) = it.next() {
        let lower = t.to_ascii_lowercase();
        match lower.as_str() {
            "-c" => {
                count = Some(it.next().filter(|n| is_number(n))?.clone());
                source = Some(Posix);
            }
            "-n" | "/n" if it.peek().is_some_and(|n| is_number(n)) => {
                count = it.next().cloned();
                source = Some(Cmd);
            }
            "-t" | "/t" if !it.peek().is_some_and(|n| is_number(n)) => {
                forever = true;
                source = Some(Cmd);
            }
            _ if t.starts_with('-') || (t.starts_with('/') && t.len() == 2) => return None,
            _ => hosts.push(t.clone()),
        }
    }
    let [host] = hosts.as_slice() else {
        return None;
    };
    let source = source.unwrap_or(native);
    if count.is_none() && !forever && source == Posix {
        forever = true;
    }
    Some((
        source,
        Intent::Ping {
            count,
            forever,
            host: host.clone(),
        },
    ))
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
            // `dir /s /b *.txt` is really "find files named *.txt".
            if a.sw("S") && a.sw("B") && !a.sw("A") {
                if let [p] = a.pos.as_slice() {
                    if has_glob(p) {
                        let (root, pattern) = split_parent(p);
                        return Some((
                            Cmd,
                            Intent::FindName {
                                root,
                                pattern,
                                ignore_case: true,
                            },
                        ));
                    }
                }
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
        "find" => {
            // cmd's `find /c /v ""` is the classic line counter.
            if (3..=4).contains(&rest.len())
                && rest[0].eq_ignore_ascii_case("/c")
                && rest[1].eq_ignore_ascii_case("/v")
                && rest[2].is_empty()
            {
                return Some((
                    Cmd,
                    Intent::CountLines {
                        path: rest.get(3).cloned(),
                    },
                ));
            }
            // Otherwise only POSIX `find [root] -name pattern`; cmd's other
            // `find` uses are text searches and are left alone.
            let (root, args) = match rest.split_first() {
                Some((r, tail)) if !r.starts_with('-') => (r.clone(), tail),
                _ => (".".to_string(), rest),
            };
            match args {
                [flag, pattern] if flag == "-name" || flag == "-iname" => Some((
                    Posix,
                    Intent::FindName {
                        root,
                        pattern: pattern.clone(),
                        ignore_case: flag == "-iname",
                    },
                )),
                _ => None,
            }
        }

        // ---- reading files -------------------------------------------------
        "head" | "tail" => {
            let (lines, follow, path) = parse_head_tail(rest, name == "tail")?;
            let intent = if name == "head" {
                Intent::Head { lines, path }
            } else {
                Intent::Tail {
                    lines,
                    follow,
                    path,
                }
            };
            Some((Posix, intent))
        }
        "wc" => match rest {
            [flag] if flag == "-l" => Some((Posix, Intent::CountLines { path: None })),
            [flag, path] if flag == "-l" && !path.starts_with('-') => Some((
                Posix,
                Intent::CountLines {
                    path: Some(path.clone()),
                },
            )),
            _ => None,
        },

        // ---- system --------------------------------------------------------
        "uname" if rest.is_empty() || rest == ["-a"] => Some((Posix, Intent::OsInfo)),
        "ver" if rest.is_empty() => Some((Cmd, Intent::OsInfo)),
        "df" if rest.is_empty() || rest == ["-h"] => Some((Posix, Intent::DiskFree)),
        "ln" => {
            let a = posix_args(rest);
            if !a.has('s') || !a.only("s", &[], &[]) {
                return None;
            }
            let (target, link) = two(a.pos)?;
            Some((
                Posix,
                Intent::Symlink {
                    dir: None,
                    target,
                    link,
                },
            ))
        }
        "mklink" => {
            let a = cmd_args(rest);
            if !a.only("", &[], &["D"]) {
                return None;
            }
            let dir = a.sw("D");
            let (link, target) = two(a.pos)?;
            Some((
                Cmd,
                Intent::Symlink {
                    dir: Some(dir),
                    target,
                    link,
                },
            ))
        }
        "open" | "xdg-open" => match rest {
            [p] if !p.starts_with('-') => Some((Posix, Intent::Open { path: p.clone() })),
            _ => None,
        },
        "start" => match rest {
            [p] | [_, p] if looks_openable(p) && (rest.len() == 1 || rest[0].is_empty()) => {
                Some((Cmd, Intent::Open { path: p.clone() }))
            }
            _ => None,
        },
        "explorer" => match rest {
            [p] if !p.starts_with('/') => Some((Cmd, Intent::Open { path: p.clone() })),
            _ => None,
        },

        // ---- environment, time, network ------------------------------------
        "env" | "printenv" if rest.is_empty() => Some((Posix, Intent::ListEnv)),
        "set" if rest.is_empty() => Some((Cmd, Intent::ListEnv)),
        "ping" => parse_ping(rest, native),
        "sleep" => match rest {
            [s] if is_number(s) => Some((Posix, Intent::Sleep { secs: s.clone() })),
            _ => None,
        },
        "timeout" => {
            let mut secs = None;
            let mut it = rest.iter();
            while let Some(t) = it.next() {
                match t.to_ascii_lowercase().as_str() {
                    "/t" | "-t" => secs = Some(it.next().filter(|n| is_number(n))?.clone()),
                    "/nobreak" | "-nobreak" => {}
                    _ => return None,
                }
            }
            Some((Cmd, Intent::Sleep { secs: secs? }))
        }
        "touch" => {
            let a = posix_args(rest);
            if !a.only("", &[], &[]) {
                return None;
            }
            Some((
                Posix,
                Intent::Touch {
                    paths: non_empty(a.pos)?,
                },
            ))
        }
        "echo" => {
            // Same command everywhere; the variable syntax gives the dialect away.
            let source = var_dialect(rest).unwrap_or(native);
            Some((
                source,
                Intent::Echo {
                    words: rest.to_vec(),
                },
            ))
        }
        "cd" | "chdir" => {
            let mut cmd_style = false;
            let args: Vec<&String> = rest
                .iter()
                .filter(|a| {
                    let d = a.eq_ignore_ascii_case("/d");
                    cmd_style |= d;
                    !d
                })
                .collect();
            let [path] = args.as_slice() else {
                return None;
            };
            let source = if cmd_style {
                Cmd
            } else {
                var_dialect(&[path.to_string()]).unwrap_or(native)
            };
            Some((
                source,
                Intent::ChangeDir {
                    path: path.to_string(),
                },
            ))
        }

        other => parse_powershell(other, rest).map(|i| (PowerShell, i)),
    }
}
