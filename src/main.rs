mod dialect;
mod emit;
mod intent;
mod parse;
mod tokenize;
mod translate;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use dialect::Dialect;
use translate::{translate, Translation};

const USAGE: &str = "\
polysh - one shell, every dialect

USAGE:
    polysh                   Start the interactive shell
    polysh -c <command>      Translate and run one command
    polysh -n <command>      Explain only: print the translation, run nothing
    polysh -h | --help       Show this help
    polysh -V | --version    Show the version

Type Windows (dir, del, copy, type, ...) or Linux/macOS (ls, rm, cp, cat, ...)
commands; polysh rewrites them for the OS you're on.";

const REPL_HELP: &str = "\
Built-ins:  cd [dir]   pwd   help   exit [code]
Anything else is translated to your OS's shell if needed, then run.
Pipes and redirection (| > < && ;) are passed through unchanged.";

enum Control {
    Continue(i32),
    Exit(i32),
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let target = Dialect::native();

    match args.first().map(String::as_str) {
        None => repl(target),
        Some("-c") => {
            let line = args[1..].join(" ");
            let code = match run_line(&line, target) {
                Control::Continue(c) | Control::Exit(c) => c,
            };
            exit_code(code)
        }
        Some("-n") | Some("--explain") => {
            let line = args[1..].join(" ");
            match translate(&line, target) {
                Translation::Translated { from, command } => {
                    println!("{command}");
                    eprintln!("({} → {})", from.name(), target.name());
                }
                Translation::Native => println!("{line}"),
                Translation::Untranslated(reason) => {
                    println!("{line}");
                    eprintln!("(no translation{})", reason.map(|r| format!(": {r}")).unwrap_or_default());
                }
            }
            ExitCode::SUCCESS
        }
        Some("-h") | Some("--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("-V") | Some("--version") => {
            println!("polysh {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("polysh: unknown option '{other}'\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

fn repl(target: Dialect) -> ExitCode {
    println!(
        "polysh {} ({}). Type 'help' for help, 'exit' to quit.",
        env!("CARGO_PKG_VERSION"),
        target.name()
    );
    let stdin = io::stdin();
    let mut last = 0;
    loop {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "?".into());
        print!("polysh {cwd}> ");
        let _ = io::stdout().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                return exit_code(last);
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("polysh: {e}");
                return ExitCode::from(1);
            }
        }

        match run_line(line.trim(), target) {
            Control::Continue(c) => last = c,
            Control::Exit(c) => return exit_code(c),
        }
    }
}

fn run_line(line: &str, target: Dialect) -> Control {
    let line = line.trim();
    if line.is_empty() {
        return Control::Continue(0);
    }
    if let Some(ctrl) = builtin(line) {
        return ctrl;
    }

    let command = match translate(line, target) {
        Translation::Native => line.to_string(),
        Translation::Translated { from, command } => {
            note(&format!("↳ {command}   ({} → {})", from.name(), target.name()));
            command
        }
        Translation::Untranslated(reason) => {
            if let Some(r) = reason {
                note(&format!("↳ running as-is ({r})"));
            }
            line.to_string()
        }
    };
    Control::Continue(execute(&command))
}

/// Commands that must run inside polysh itself (a child process can't
/// change our working directory) or that every OS should have.
fn builtin(line: &str) -> Option<Control> {
    let tokens = tokenize::tokenize(line)?;
    let (first, rest) = tokens.split_first()?;
    match first.to_ascii_lowercase().as_str() {
        "exit" | "quit" => {
            let code = rest.first().and_then(|c| c.parse().ok()).unwrap_or(0);
            Some(Control::Exit(code))
        }
        "help" if rest.is_empty() => {
            println!("{REPL_HELP}");
            Some(Control::Continue(0))
        }
        "pwd" if rest.is_empty() => {
            match std::env::current_dir() {
                Ok(p) => println!("{}", p.display()),
                Err(e) => eprintln!("pwd: {e}"),
            }
            Some(Control::Continue(0))
        }
        "cd" | "chdir" => {
            // cmd's `cd /d D:\x` switches drive too; std handles that for us.
            let args: Vec<&String> = rest.iter().filter(|a| !a.eq_ignore_ascii_case("/d")).collect();
            let dest = match args.as_slice() {
                [] => home_dir(),
                [p] => Some(expand_home(p)),
                _ => {
                    eprintln!("cd: too many arguments");
                    return Some(Control::Continue(1));
                }
            };
            let Some(dest) = dest else {
                eprintln!("cd: no home directory found");
                return Some(Control::Continue(1));
            };
            match std::env::set_current_dir(&dest) {
                Ok(()) => Some(Control::Continue(0)),
                Err(e) => {
                    eprintln!("cd: {}: {e}", dest.display());
                    Some(Control::Continue(1))
                }
            }
        }
        _ => None,
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn expand_home(p: &str) -> PathBuf {
    if p == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(p));
    }
    if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}

/// Print a dim status line to stderr (plain text if NO_COLOR is set).
fn note(msg: &str) {
    if std::env::var_os("NO_COLOR").is_some() {
        eprintln!("  {msg}");
    } else {
        eprintln!("\x1b[2m  {msg}\x1b[0m");
    }
}

fn execute(command: &str) -> i32 {
    match system_shell(command).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("polysh: failed to start the system shell: {e}");
            127
        }
    }
}

#[cfg(windows)]
fn system_shell(command: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new("cmd");
    // /S + outer quotes makes cmd take the command line verbatim.
    c.args(["/D", "/S", "/C"]).raw_arg(format!("\"{command}\""));
    c
}

#[cfg(not(windows))]
fn system_shell(command: &str) -> std::process::Command {
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(command);
    c
}
