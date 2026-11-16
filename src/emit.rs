//! [`Intent`] → command line in a target [`Dialect`].

use crate::dialect::Dialect::{self, Cmd, Posix, PowerShell};
use crate::intent::Intent;
use crate::vars;

/// Write `intent` in the `to` dialect. `from` is the dialect it was parsed
/// from, used to translate environment variables in arguments.
pub fn emit(intent: &Intent, from: Dialect, to: Dialect) -> String {
    let l = Ctx { from, to };
    match to {
        Cmd => emit_cmd(intent, l),
        Posix => emit_posix(intent, l),
        PowerShell => emit_powershell(intent, l),
    }
}

#[derive(Clone, Copy)]
struct Ctx {
    from: Dialect,
    to: Dialect,
}

impl Ctx {
    fn line(self, program: &str) -> Line {
        Line {
            parts: vec![program.to_string()],
            ctx: self,
        }
    }

    /// An argument that names a file: variables and separators converted,
    /// then quoted if needed.
    fn arg(self, p: &str) -> String {
        quote(&self.convert(p), self.to)
    }

    fn convert(self, p: &str) -> String {
        let p = vars::convert(p, self.from, self.to);
        match self.to {
            Posix => p.replace('\\', "/"),
            Cmd | PowerShell => p.replace('/', "\\"),
        }
    }
}

/// Small builder for one command line.
struct Line {
    parts: Vec<String>,
    ctx: Ctx,
}

impl Line {
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
        let s: String = flags
            .iter()
            .filter(|(_, on)| *on)
            .map(|(c, _)| *c)
            .collect();
        if s.is_empty() {
            self
        } else {
            self.word(&format!("-{s}"))
        }
    }

    fn path(mut self, p: &str) -> Self {
        let a = self.ctx.arg(p);
        self.parts.push(a);
        self
    }

    /// Several paths. PowerShell needs them comma-separated.
    fn paths(mut self, ps: &[String]) -> Self {
        if self.ctx.to == PowerShell && ps.len() > 1 {
            let list: Vec<String> = ps.iter().map(|p| self.ctx.arg(p)).collect();
            self.parts.push(list.join(", "));
            return self;
        }
        for p in ps {
            self = self.path(p);
        }
        self
    }

    /// A search pattern: always quoted, never variable-expanded.
    fn literal(mut self, s: &str) -> Self {
        let q = match self.ctx.to {
            Posix => format!("'{}'", s.replace('\'', r"'\''")),
            Cmd => format!("\"{}\"", s.replace('"', "\"\"")),
            PowerShell => format!("'{}'", s.replace('\'', "''")),
        };
        self.parts.push(q);
        self
    }

    fn build(self) -> String {
        self.parts.join(" ")
    }
}

fn quote(s: &str, to: Dialect) -> String {
    let needs = s.is_empty()
        || s.chars().any(|c| {
            c.is_whitespace()
                || matches!(
                    c,
                    '"' | '\'' | '`' | '&' | '|' | '<' | '>' | ';' | '(' | ')' | '^'
                )
                || (to == PowerShell && matches!(c, '{' | '}' | '@' | '#' | ','))
        });
    if !needs {
        return s.to_string();
    }
    match to {
        // Double quotes keep `$HOME` expanding; single quotes are fully literal.
        Posix if s.contains('$') => format!(
            "\"{}\"",
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
        ),
        Posix => format!("'{}'", s.replace('\'', r"'\''")),
        Cmd => format!("\"{}\"", s.replace('"', "\"\"")),
        PowerShell if s.contains('$') => {
            format!("\"{}\"", s.replace('`', "``").replace('"', "`\""))
        }
        PowerShell => format!("'{}'", s.replace('\'', "''")),
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

fn with_exe(name: &str) -> String {
    if name.contains('.') {
        name.to_string()
    } else {
        format!("{name}.exe")
    }
}

/// cmd has no built-in for this; run the PowerShell version instead.
fn via_powershell(intent: &Intent, l: Ctx) -> String {
    let ps = emit_powershell(
        intent,
        Ctx {
            from: l.from,
            to: PowerShell,
        },
    );
    let reads_input = matches!(
        intent,
        Intent::Head { path: None, .. }
            | Intent::Tail { path: None, .. }
            | Intent::CountLines { path: None }
    );
    let ps = if reads_input {
        format!("$input | {ps}")
    } else {
        ps
    };
    format!(
        "powershell -NoProfile -Command \"{}\"",
        ps.replace('"', "\\\"")
    )
}

fn emit_cmd(intent: &Intent, l: Ctx) -> String {
    match intent {
        // cmd's own `find /c /v ""` would do for counting, but a Git or MSYS
        // `find` earlier in PATH turns it into a whole-disk walk.
        Intent::Head { .. }
        | Intent::Tail { .. }
        | Intent::CountLines { .. }
        | Intent::DiskFree => via_powershell(intent, l),
        Intent::OsInfo => "ver".to_string(),
        Intent::Symlink {
            dir: Some(dir),
            target,
            link,
        } => l
            .line("mklink")
            .flag(*dir, "/D")
            .path(link)
            .path(target)
            .build(),
        Intent::Symlink {
            dir: None,
            target,
            link,
        } => {
            let (t, k) = (l.convert(target), l.convert(link));
            format!("if exist \"{t}\\*\" (mklink /D \"{k}\" \"{t}\") else (mklink \"{k}\" \"{t}\")")
        }
        Intent::Open { path } => l.line("start").word("\"\"").path(path).build(),
        Intent::ListDir {
            all,
            long,
            recursive,
            paths,
        } => l
            .line("dir")
            .flag(*all, "/A")
            .flag(*recursive, "/S")
            .flag(!*long, "/B")
            .paths(paths)
            .build(),
        Intent::FindName { root, pattern, .. } => {
            let target = if root == "." {
                pattern.clone()
            } else {
                format!("{root}\\{pattern}")
            };
            l.line("dir").word("/S").word("/B").path(&target).build()
        }
        Intent::Copy {
            recursive: true,
            src,
            dst,
        } => l
            .line("xcopy")
            .path(src)
            .path(dst)
            .word("/E")
            .word("/I")
            .word("/Y")
            .build(),
        Intent::Copy { src, dst, .. } => l.line("copy").word("/Y").path(src).path(dst).build(),
        Intent::Move { src, dst } => l.line("move").word("/Y").path(src).path(dst).build(),
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
                    let p = l.convert(p);
                    format!("if exist \"{p}\\*\" (rmdir /S /Q \"{p}\") else ({del} \"{p}\")")
                })
                .collect::<Vec<_>>()
                .join(" & ")
        }
        Intent::Remove { force, paths, .. } => l
            .line("del")
            .flag(*force, "/F")
            .flag(*force, "/Q")
            .paths(paths)
            .build(),
        Intent::MakeDir { paths } => l.line("mkdir").paths(paths).build(),
        Intent::RemoveDir { recursive, paths } => l
            .line("rmdir")
            .flag(*recursive, "/S")
            .flag(*recursive, "/Q")
            .paths(paths)
            .build(),
        // Appending nothing creates the file without touching its contents.
        Intent::Touch { paths } => paths
            .iter()
            .map(|p| format!("type nul >> \"{}\"", l.convert(p)))
            .collect::<Vec<_>>()
            .join(" & "),
        Intent::PrintFile { paths } => l.line("type").paths(paths).build(),
        Intent::Echo { words } if words.is_empty() => "echo.".to_string(),
        Intent::Echo { words } => {
            let text: Vec<String> = words
                .iter()
                .map(|w| vars::convert(w, l.from, l.to))
                .collect();
            format!("echo {}", text.join(" "))
        }
        Intent::ChangeDir { path } => l.line("cd").word("/d").path(path).build(),
        Intent::Clear => "cls".to_string(),
        Intent::ListEnv => "set".to_string(),
        Intent::NetConfig { all } => l.line("ipconfig").flag(*all, "/all").build(),
        Intent::Ping {
            count,
            forever,
            host,
        } => {
            let line = l.line("ping");
            let line = match (count, forever) {
                (Some(n), _) => line.word("-n").word(n),
                (None, true) => line.word("-t"),
                (None, false) => line,
            };
            line.path(host).build()
        }
        Intent::Sleep { secs } => format!("timeout /t {secs} /nobreak >nul"),
        Intent::ProcessList => "tasklist".to_string(),
        Intent::KillPid { force, pids } => {
            let mut line = l.line("taskkill").flag(*force, "/F");
            for pid in pids {
                line = line.word("/PID").word(pid);
            }
            line.build()
        }
        Intent::KillName { force, name } => l
            .line("taskkill")
            .flag(*force, "/F")
            .word("/IM")
            .path(&with_exe(name))
            .build(),
        Intent::Which { names } => l.line("where").paths(names).build(),
        Intent::Search {
            ignore_case,
            recursive,
            line_numbers,
            pattern,
            paths,
        } => {
            let line = l
                .line("findstr")
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

fn emit_posix(intent: &Intent, l: Ctx) -> String {
    match intent {
        Intent::Head { lines, path } => {
            let line = l.line("head").word("-n").word(lines);
            match path {
                Some(p) => line.path(p).build(),
                None => line.build(),
            }
        }
        Intent::Tail {
            lines,
            follow,
            path,
        } => {
            let line = l.line("tail").word("-n").word(lines).flag(*follow, "-f");
            match path {
                Some(p) => line.path(p).build(),
                None => line.build(),
            }
        }
        // `< file` so only the number is printed, like the other dialects.
        Intent::CountLines { path: None } => "wc -l".to_string(),
        Intent::CountLines { path: Some(p) } => format!("wc -l < {}", l.arg(p)),
        Intent::OsInfo => "uname -a".to_string(),
        Intent::DiskFree => "df -h".to_string(),
        Intent::Symlink { target, link, .. } => {
            l.line("ln").word("-s").path(target).path(link).build()
        }
        Intent::Open { path } => {
            let opener = if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            };
            l.line(opener).path(path).build()
        }
        Intent::ListDir {
            all,
            long,
            recursive,
            paths,
        } => l
            .line("ls")
            .bundle(&[('a', *all), ('l', *long), ('R', *recursive)])
            .paths(paths)
            .build(),
        Intent::FindName {
            root,
            pattern,
            ignore_case,
        } => l
            .line("find")
            .path(root)
            .word(if *ignore_case { "-iname" } else { "-name" })
            .literal(pattern)
            .build(),
        Intent::Copy {
            recursive,
            src,
            dst,
        } => l
            .line("cp")
            .flag(*recursive, "-r")
            .path(src)
            .path(dst)
            .build(),
        Intent::Move { src, dst } => l.line("mv").path(src).path(dst).build(),
        Intent::Remove {
            recursive,
            force,
            paths,
        } => l
            .line("rm")
            .bundle(&[('r', *recursive), ('f', *force)])
            .paths(paths)
            .build(),
        Intent::MakeDir { paths } => l.line("mkdir").word("-p").paths(paths).build(),
        Intent::RemoveDir {
            recursive: true,
            paths,
        } => l.line("rm").word("-r").paths(paths).build(),
        Intent::RemoveDir { paths, .. } => l.line("rmdir").paths(paths).build(),
        Intent::Touch { paths } => l.line("touch").paths(paths).build(),
        Intent::PrintFile { paths } => l.line("cat").paths(paths).build(),
        Intent::Echo { words } => {
            let text: Vec<String> = words
                .iter()
                .map(|w| vars::convert(w, l.from, l.to))
                .collect();
            l.line("echo").word(&quote(&text.join(" "), Posix)).build()
        }
        Intent::ChangeDir { path } => l.line("cd").path(path).build(),
        Intent::Clear => "clear".to_string(),
        Intent::ListEnv => "env".to_string(),
        Intent::NetConfig { .. } => {
            if cfg!(target_os = "linux") {
                "ip addr".to_string()
            } else {
                "ifconfig".to_string()
            }
        }
        Intent::Ping {
            count,
            forever,
            host,
        } => {
            let line = l.line("ping");
            let line = match (count, forever) {
                (Some(n), _) => line.word("-c").word(n),
                (None, true) => line,
                // Windows' default is 4; POSIX would otherwise ping forever.
                (None, false) => line.word("-c").word("4"),
            };
            line.path(host).build()
        }
        Intent::Sleep { secs } => format!("sleep {secs}"),
        Intent::ProcessList => "ps aux".to_string(),
        Intent::KillPid { force, pids } => l.line("kill").flag(*force, "-9").paths(pids).build(),
        Intent::KillName { force, name } => l
            .line("pkill")
            .flag(*force, "-9")
            .path(strip_exe(name))
            .build(),
        Intent::Which { names } => l.line("which").paths(names).build(),
        Intent::Search {
            ignore_case,
            recursive,
            line_numbers,
            pattern,
            paths,
        } => l
            .line("grep")
            .bundle(&[('i', *ignore_case), ('r', *recursive), ('n', *line_numbers)])
            .literal(pattern)
            .paths(paths)
            .build(),
    }
}

fn emit_powershell(intent: &Intent, l: Ctx) -> String {
    match intent {
        Intent::Head {
            lines,
            path: Some(p),
        } => l
            .line("Get-Content")
            .word("-TotalCount")
            .word(lines)
            .path(p)
            .build(),
        Intent::Head { lines, path: None } => format!("Select-Object -First {lines}"),
        Intent::Tail {
            lines,
            follow,
            path: Some(p),
        } => l
            .line("Get-Content")
            .word("-Tail")
            .word(lines)
            .flag(*follow, "-Wait")
            .path(p)
            .build(),
        Intent::Tail {
            lines, path: None, ..
        } => format!("Select-Object -Last {lines}"),
        // Not `Measure-Object -Line`: that skips blank lines, `wc -l` doesn't.
        Intent::CountLines { path: None } => {
            "Measure-Object | Select-Object -ExpandProperty Count".to_string()
        }
        Intent::CountLines { path: Some(p) } => format!("@(Get-Content {}).Count", l.arg(p)),
        Intent::OsInfo => "[System.Environment]::OSVersion.VersionString".to_string(),
        Intent::DiskFree => "Get-PSDrive -PSProvider FileSystem".to_string(),
        Intent::Symlink { target, link, .. } => l
            .line("New-Item")
            .word("-ItemType")
            .word("SymbolicLink")
            .word("-Path")
            .path(link)
            .word("-Target")
            .path(target)
            .build(),
        Intent::Open { path } => l.line("Start-Process").path(path).build(),
        Intent::ListDir {
            all,
            long,
            recursive,
            paths,
        } => l
            .line("Get-ChildItem")
            .flag(*all, "-Force")
            .flag(*recursive, "-Recurse")
            .flag(!*long, "-Name")
            .paths(paths)
            .build(),
        Intent::FindName { root, pattern, .. } => {
            let line = l.line("Get-ChildItem");
            let line = if root == "." { line } else { line.path(root) };
            line.word("-Recurse")
                .word("-Filter")
                .literal(pattern)
                .word("-Name")
                .build()
        }
        Intent::Copy {
            recursive,
            src,
            dst,
        } => l
            .line("Copy-Item")
            .path(src)
            .path(dst)
            .flag(*recursive, "-Recurse")
            .build(),
        Intent::Move { src, dst } => l.line("Move-Item").path(src).path(dst).build(),
        Intent::Remove {
            recursive,
            force,
            paths,
        } => l
            .line("Remove-Item")
            .flag(*recursive, "-Recurse")
            .flag(*force, "-Force")
            .paths(paths)
            .build(),
        Intent::MakeDir { paths } => l
            .line("New-Item")
            .word("-ItemType")
            .word("Directory")
            .word("-Force")
            .paths(paths)
            .build(),
        Intent::RemoveDir { recursive, paths } => l
            .line("Remove-Item")
            .flag(*recursive, "-Recurse")
            .paths(paths)
            .build(),
        Intent::Touch { paths } => paths
            .iter()
            .map(|p| {
                let a = l.arg(p);
                format!(
                    "if (Test-Path {a}) {{ (Get-Item {a}).LastWriteTime = Get-Date }} else {{ New-Item -ItemType File {a} | Out-Null }}"
                )
            })
            .collect::<Vec<_>>()
            .join("; "),
        Intent::PrintFile { paths } => l.line("Get-Content").paths(paths).build(),
        Intent::Echo { words } => {
            let text: Vec<String> = words
                .iter()
                .map(|w| vars::convert(w, l.from, l.to))
                .collect();
            l.line("Write-Output")
                .word(&quote(&text.join(" "), PowerShell))
                .build()
        }
        Intent::ChangeDir { path } => l.line("Set-Location").path(path).build(),
        Intent::Clear => "Clear-Host".to_string(),
        Intent::ListEnv => "Get-ChildItem Env:".to_string(),
        Intent::NetConfig { .. } => "Get-NetIPConfiguration".to_string(),
        Intent::Ping {
            count,
            forever,
            host,
        } => match (count, forever) {
            (Some(n), _) => l
                .line("Test-Connection")
                .path(host)
                .word("-Count")
                .word(n)
                .build(),
            (None, true) => l.line("ping").word("-t").path(host).build(),
            (None, false) => l.line("Test-Connection").path(host).build(),
        },
        Intent::Sleep { secs } => format!("Start-Sleep -Seconds {secs}"),
        Intent::ProcessList => "Get-Process".to_string(),
        Intent::KillPid { force, pids } => l
            .line("Stop-Process")
            .word("-Id")
            .word(&pids.join(", "))
            .flag(*force, "-Force")
            .build(),
        Intent::KillName { force, name } => l
            .line("Stop-Process")
            .word("-Name")
            .path(strip_exe(name))
            .flag(*force, "-Force")
            .build(),
        Intent::Which { names } => l.line("Get-Command").paths(names).build(),
        Intent::Search {
            ignore_case,
            recursive,
            pattern,
            paths,
            ..
        } => {
            let sls = |line: Line| {
                line.word("-Pattern")
                    .literal(pattern)
                    .flag(!*ignore_case, "-CaseSensitive")
            };
            if *recursive {
                let gci = l.line("Get-ChildItem").word("-Recurse").word("-File");
                let gci = if paths.is_empty() { gci } else { gci.paths(paths) };
                format!("{} | {}", gci.build(), sls(l.line("Select-String")).build())
            } else {
                let line = sls(l.line("Select-String"));
                let line = if paths.is_empty() {
                    line
                } else {
                    line.word("-Path").paths(paths)
                };
                line.build()
            }
        }
    }
}
