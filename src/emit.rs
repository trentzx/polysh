//! [`Intent`] → command line in a target [`Dialect`].

use crate::dialect::Dialect;
use crate::intent::Intent;

pub fn emit(intent: &Intent, target: Dialect) -> String {
    match target {
        Dialect::Cmd => emit_cmd(intent),
        Dialect::Posix => emit_posix(intent),
    }
}

/// Small builder that quotes and converts arguments for one dialect.
struct Line {
    parts: Vec<String>,
    target: Dialect,
}

impl Line {
    fn new(target: Dialect, program: &str) -> Self {
        Line {
            parts: vec![program.to_string()],
            target,
        }
    }

    fn word(mut self, w: &str) -> Self {
        self.parts.push(w.to_string());
        self
    }

    fn flag(self, on: bool, f: &str) -> Self {
        if on {
            self.word(f)
        } else {
            self
        }
    }

    /// POSIX-style bundled flags, e.g. `[('a', true), ('l', false)]` → `-a`.
    fn bundle(self, flags: &[(char, bool)]) -> Self {
        let s: String = flags.iter().filter(|(_, on)| *on).map(|(c, _)| *c).collect();
        if s.is_empty() {
            self
        } else {
            self.word(&format!("-{s}"))
        }
    }

    fn path(mut self, p: &str) -> Self {
        let converted = convert_path(p, self.target);
        self.parts.push(quote(&converted, self.target, false));
        self
    }

    fn paths(mut self, ps: &[String]) -> Self {
        for p in ps {
            self = self.path(p);
        }
        self
    }

    /// An argument that's always quoted (search patterns).
    fn literal(mut self, s: &str) -> Self {
        self.parts.push(quote(s, self.target, true));
        self
    }

    fn build(self) -> String {
        self.parts.join(" ")
    }
}

fn convert_path(p: &str, target: Dialect) -> String {
    match target {
        Dialect::Posix => p.replace('\\', "/"),
        Dialect::Cmd => p.replace('/', "\\"),
    }
}

fn quote(s: &str, target: Dialect, force: bool) -> String {
    let needs = force
        || s.is_empty()
        || s.chars().any(|c| {
            c.is_whitespace()
                || matches!(c, '"' | '\'' | '$' | '`' | '&' | '|' | '<' | '>' | ';' | '(' | ')' | '^')
        });
    if !needs {
        return s.to_string();
    }
    match target {
        Dialect::Posix => format!("'{}'", s.replace('\'', r"'\''")),
        Dialect::Cmd => format!("\"{}\"", s.replace('"', "\"\"")),
    }
}

fn strip_exe(name: &str) -> &str {
    match name.len().checked_sub(4) {
        Some(i) if i > 0 && name.is_char_boundary(i) && name[i..].eq_ignore_ascii_case(".exe") => {
            &name[..i]
        }
        _ => name,
    }
}

fn emit_cmd(intent: &Intent) -> String {
    let t = Dialect::Cmd;
    match intent {
        Intent::ListDir {
            all,
            long,
            recursive,
            paths,
        } => Line::new(t, "dir")
            .flag(*all, "/A")
            .flag(*recursive, "/S")
            .flag(!*long, "/B")
            .paths(paths)
            .build(),
        Intent::Copy {
            recursive: true,
            src,
            dst,
        } => Line::new(t, "xcopy")
            .path(src)
            .path(dst)
            .word("/E")
            .word("/I")
            .word("/Y")
            .build(),
        Intent::Copy { src, dst, .. } => Line::new(t, "copy").word("/Y").path(src).path(dst).build(),
        Intent::Move { src, dst } => Line::new(t, "move").word("/Y").path(src).path(dst).build(),
        Intent::Remove {
            recursive: true,
            force,
            paths,
        } => {
            // `rm -r` works on files *and* folders; cmd needs a different
            // command for each, so pick at run time.
            let del = if *force { "del /F /Q" } else { "del /Q" };
            paths
                .iter()
                .map(|p| {
                    let p = convert_path(p, t);
                    format!("if exist \"{p}\\*\" (rmdir /S /Q \"{p}\") else ({del} \"{p}\")")
                })
                .collect::<Vec<_>>()
                .join(" & ")
        }
        Intent::Remove { force, paths, .. } => Line::new(t, "del")
            .flag(*force, "/F")
            .flag(*force, "/Q")
            .paths(paths)
            .build(),
        Intent::MakeDir { paths } => Line::new(t, "mkdir").paths(paths).build(),
        Intent::RemoveDir { recursive, paths } => Line::new(t, "rmdir")
            .flag(*recursive, "/S")
            .flag(*recursive, "/Q")
            .paths(paths)
            .build(),
        Intent::PrintFile { paths } => Line::new(t, "type").paths(paths).build(),
        Intent::Clear => "cls".to_string(),
        Intent::NetConfig { all } => Line::new(t, "ipconfig").flag(*all, "/all").build(),
        Intent::ProcessList => "tasklist".to_string(),
        Intent::KillPid { force, pids } => {
            let mut line = Line::new(t, "taskkill").flag(*force, "/F");
            for pid in pids {
                line = line.word("/PID").word(pid);
            }
            line.build()
        }
        Intent::KillName { force, name } => {
            let image = if name.contains('.') {
                name.clone()
            } else {
                format!("{name}.exe")
            };
            Line::new(t, "taskkill")
                .flag(*force, "/F")
                .word("/IM")
                .path(&image)
                .build()
        }
        Intent::Which { names } => Line::new(t, "where").paths(names).build(),
        Intent::Search {
            ignore_case,
            recursive,
            line_numbers,
            pattern,
            paths,
        } => {
            let line = Line::new(t, "findstr")
                .flag(*ignore_case, "/I")
                .flag(*recursive, "/S")
                .flag(*line_numbers, "/N")
                .literal(pattern);
            if paths.is_empty() && *recursive {
                // `grep -r pat` searches the current folder; findstr needs a file mask.
                line.word("*").build()
            } else {
                line.paths(paths).build()
            }
        }
    }
}

fn emit_posix(intent: &Intent) -> String {
    let t = Dialect::Posix;
    match intent {
        Intent::ListDir {
            all,
            long,
            recursive,
            paths,
        } => Line::new(t, "ls")
            .bundle(&[('a', *all), ('l', *long), ('R', *recursive)])
            .paths(paths)
            .build(),
        Intent::Copy {
            recursive,
            src,
            dst,
        } => Line::new(t, "cp")
            .flag(*recursive, "-r")
            .path(src)
            .path(dst)
            .build(),
        Intent::Move { src, dst } => Line::new(t, "mv").path(src).path(dst).build(),
        Intent::Remove {
            recursive,
            force,
            paths,
        } => Line::new(t, "rm")
            .bundle(&[('r', *recursive), ('f', *force)])
            .paths(paths)
            .build(),
        Intent::MakeDir { paths } => Line::new(t, "mkdir").word("-p").paths(paths).build(),
        Intent::RemoveDir {
            recursive: true,
            paths,
        } => Line::new(t, "rm").word("-r").paths(paths).build(),
        Intent::RemoveDir { paths, .. } => Line::new(t, "rmdir").paths(paths).build(),
        Intent::PrintFile { paths } => Line::new(t, "cat").paths(paths).build(),
        Intent::Clear => "clear".to_string(),
        Intent::NetConfig { .. } => {
            if cfg!(target_os = "linux") {
                "ip addr".to_string()
            } else {
                "ifconfig".to_string()
            }
        }
        Intent::ProcessList => "ps aux".to_string(),
        Intent::KillPid { force, pids } => Line::new(t, "kill")
            .flag(*force, "-9")
            .paths(pids)
            .build(),
        Intent::KillName { force, name } => Line::new(t, "pkill")
            .flag(*force, "-9")
            .path(strip_exe(name))
            .build(),
        Intent::Which { names } => Line::new(t, "which").paths(names).build(),
        Intent::Search {
            ignore_case,
            recursive,
            line_numbers,
            pattern,
            paths,
        } => Line::new(t, "grep")
            .bundle(&[('i', *ignore_case), ('r', *recursive), ('n', *line_numbers)])
            .literal(pattern)
            .paths(paths)
            .build(),
    }
}
