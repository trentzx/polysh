mod dialect;
mod emit;
mod init;
mod intent;
mod parse;
mod tokenize;
mod translate;
mod vars;

use std::borrow::Cow::{self, Owned};
use std::path::PathBuf;
use std::process::ExitCode;

use rustyline::completion::FilenameCompleter;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::HistoryHinter;
use rustyline::{Completer, CompletionType, Config, Editor, Helper, Hinter, Validator};

use dialect::Dialect;
use tokenize::{split_ops, split_redirect, Piece};
use translate::{translate, Translation};

const USAGE: &str = r#"polysh - one shell, every dialect

USAGE:
    polysh                          Start the interactive shell
    polysh -c <command>             Translate and run one command
    polysh -n [--to <dialect>] <command>
                                    Explain only: print the translation, run nothing
    polysh init <shell>             Print a hook that makes your own shell
                                    understand every dialect (bash, zsh, fish,
                                    powershell, cmd)
    polysh -h | --help              Show this help
    polysh -V | --version           Show the version

DIALECTS (for --to):
    cmd          Windows Command Prompt
    posix        Linux / macOS (sh, bash, zsh)
    powershell   Windows PowerShell / PowerShell 7

Type Windows (dir, del, copy, ...), Linux/macOS (ls, rm, cp, ...) or
PowerShell (Get-ChildItem, Remove-Item, ...) commands; polysh rewrites them
for the OS you're on.

SET UP YOUR OWN SHELL (unknown commands get translated, nothing else changes):
    bash        echo 'eval "$(polysh init bash)"' >> ~/.bashrc
    zsh         echo 'eval "$(polysh init zsh)"' >> ~/.zshrc
    fish        echo 'polysh init fish | source' >> ~/.config/fish/config.fish
    PowerShell  Add-Content $PROFILE 'Invoke-Expression (& polysh init powershell | Out-String)'"#;

const REPL_HELP: &str = "\
Built-ins:  cd [dir]   pwd   help   exit [code]
Anything else is translated for your OS if needed, then run. Pipes (|),
chains (&& || ; &) and redirection (> >> < 2>) are translated piece by piece.
Keys: ↑/↓ history, → accept suggestion, Tab complete paths, Ctrl+C cancel,
Ctrl+D exit.";

/// For `cd -`.
static PREVIOUS_DIR: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

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
            let line = join_args(&args[1..]);
            let code = match run_line(&line, target) {
                Control::Continue(c) | Control::Exit(c) => c,
            };
            exit_code(code)
        }
        Some("-n") | Some("--explain") => explain(&args[1..], target),
        Some("init") => match args.get(1).and_then(|s| init::snippet(s)) {
            Some(snippet) => {
                print!("{snippet}");
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("polysh: usage: polysh init <shell>   ({})", init::SHELLS);
                ExitCode::from(2)
            }
        },
        Some("--hook") => hook(&args[1..], target),
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

fn explain(args: &[String], native: Dialect) -> ExitCode {
    let (target, rest) = match args {
        [flag, name, rest @ ..] if flag == "--to" => (Dialect::from_name(name), rest),
        [flag, rest @ ..] if flag.starts_with("--to=") => (Dialect::from_name(&flag[5..]), rest),
        _ => (Some(native), args),
    };
    let Some(target) = target else {
        eprintln!("polysh: unknown dialect; use cmd, posix or powershell");
        return ExitCode::from(2);
    };
    let line = rest.join(" ");
    match translate(&line, target) {
        Translation::Translated { from, command } => {
            println!("{command}");
            eprintln!("({} → {})", from.name(), target.name());
        }
        Translation::Native => println!("{line}"),
        Translation::Untranslated(reason) => {
            println!("{line}");
            eprintln!(
                "(no translation{})",
                reason.map(|r| format!(": {r}")).unwrap_or_default()
            );
        }
    }
    ExitCode::SUCCESS
}

/// Rebuild a command line from separate arguments, quoting the ones that
/// need it. A single argument is taken as an already-written command line
/// (`polysh -c "ls -la | grep x"`).
fn join_args(args: &[String]) -> String {
    match args {
        [line] => line.clone(),
        _ => quote_words(args),
    }
}

/// Join separate words into a command line, quoting any that hold spaces
/// or shell operators so they stay one word.
fn quote_words(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            let plain = !a.is_empty()
                && !a.chars().any(|c| {
                    c.is_whitespace()
                        || matches!(
                            c,
                            '"' | '\'' | '|' | '&' | ';' | '<' | '>' | '(' | ')' | '`'
                        )
                });
            if plain {
                a.clone()
            } else if a.contains('"') {
                format!("'{a}'")
            } else {
                format!("\"{a}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Called by the `polysh init` shell hooks with a command the shell didn't
/// recognise. Runs (or with `--print`, prints) the translation; exits 127
/// without output when there is none, so the shell can report "not found".
fn hook(args: &[String], native: Dialect) -> ExitCode {
    let mut print = false;
    let mut target = native;
    let mut rest = args;
    loop {
        match rest {
            [flag, tail @ ..] if flag == "--print" => {
                print = true;
                rest = tail;
            }
            [flag, name, tail @ ..] if flag == "--to" => {
                let Some(d) = Dialect::from_name(name) else {
                    return ExitCode::from(2);
                };
                target = d;
                rest = tail;
            }
            [flag, tail @ ..] if flag == "--" => {
                rest = tail;
                break;
            }
            _ => break,
        }
    }
    // Every word arrives separately, already unquoted by the calling shell.
    match translate(&quote_words(rest), target) {
        Translation::Translated { from, command } => {
            note(&format!(
                "↳ {command}   ({} → {})",
                from.name(),
                target.name()
            ));
            if print {
                println!("{command}");
                ExitCode::SUCCESS
            } else {
                exit_code(execute(&command))
            }
        }
        _ => ExitCode::from(127),
    }
}

fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

#[derive(Helper, Completer, Hinter, Validator, Default)]
struct ShellHelper {
    #[rustyline(Completer)]
    completer: FilenameCompleter,
    #[rustyline(Hinter)]
    hinter: HistoryHinter,
}

impl Highlighter for ShellHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned(format!("\x1b[2m{hint}\x1b[0m"))
    }
}

fn history_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".polysh_history"))
}

/// The current folder, with the home folder shortened to `~`.
fn short_cwd() -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return "?".into();
    };
    if let Some(home) = home_dir() {
        if let Ok(rest) = cwd.strip_prefix(&home) {
            let rest = rest.display().to_string();
            return if rest.is_empty() {
                "~".into()
            } else {
                format!("~{}{rest}", std::path::MAIN_SEPARATOR)
            };
        }
    }
    cwd.display().to_string()
}

fn repl(target: Dialect) -> ExitCode {
    // Ctrl+C should stop the running command, not polysh. The child gets the
    // signal from the terminal; we just ignore it.
    let _ = ctrlc::set_handler(|| {});

    let config = Config::builder()
        .completion_type(CompletionType::List)
        .history_ignore_space(true)
        .auto_add_history(true)
        .max_history_size(5000)
        .unwrap_or_else(|_| Config::builder())
        .build();
    let mut rl = match Editor::<ShellHelper, _>::with_default(config) {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("polysh: can't start the line editor: {e}");
            return ExitCode::from(1);
        }
    };
    let history = history_path();
    if let Some(h) = &history {
        let _ = rl.load_history(h);
    }

    println!(
        "polysh {} · {} · type 'help' for help",
        env!("CARGO_PKG_VERSION"),
        target.name()
    );

    let mut last = 0;
    let code = loop {
        let cwd = short_cwd();
        let plain = format!("{cwd} › ");
        let arrow = if last == 0 { "32" } else { "31" };
        let colored = format!("\x1b[36m{cwd}\x1b[0m \x1b[{arrow}m›\x1b[0m ");
        match rl.readline(&(plain, colored)) {
            Ok(line) => match run_line(&line, target) {
                Control::Continue(c) => last = c,
                Control::Exit(c) => break c,
            },
            Err(ReadlineError::Interrupted) => last = 130,
            Err(ReadlineError::Eof) => break last,
            Err(e) => {
                eprintln!("polysh: {e}");
                break 1;
            }
        }
    };

    if let Some(h) = &history {
        let _ = rl.save_history(h);
    }
    exit_code(code)
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
            note(&format!(
                "↳ {command}   ({} → {})",
                from.name(),
                target.name()
            ));
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
/// change our working directory) or that every OS should have. Only applies
/// to a lone command; inside a chain, `cd` is translated and run normally.
fn builtin(line: &str) -> Option<Control> {
    match split_ops(line)?.as_slice() {
        [Piece::Command(c)] if split_redirect(c).1.is_empty() => {}
        _ => return None,
    }
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
        "pwd" | "get-location" | "gl" if rest.is_empty() => {
            match std::env::current_dir() {
                Ok(p) => println!("{}", p.display()),
                Err(e) => eprintln!("pwd: {e}"),
            }
            Some(Control::Continue(0))
        }
        "cd" | "chdir" | "set-location" | "sl" => {
            // cmd's `cd /d D:\x` switches drive too; std handles that for us.
            let args: Vec<&String> = rest
                .iter()
                .filter(|a| !a.eq_ignore_ascii_case("/d") && !a.eq_ignore_ascii_case("-path"))
                .collect();
            let dest = match args.as_slice() {
                [] => home_dir(),
                [p] if p.as_str() == "-" => {
                    match PREVIOUS_DIR.lock().ok().and_then(|g| g.clone()) {
                        Some(prev) => {
                            println!("{}", prev.display());
                            Some(prev)
                        }
                        None => {
                            eprintln!("cd: no previous directory");
                            return Some(Control::Continue(1));
                        }
                    }
                }
                [p] => Some(PathBuf::from(vars::expand(p))),
                _ => {
                    eprintln!("cd: too many arguments");
                    return Some(Control::Continue(1));
                }
            };
            let Some(dest) = dest else {
                eprintln!("cd: no home directory found");
                return Some(Control::Continue(1));
            };
            let before = std::env::current_dir().ok();
            match std::env::set_current_dir(&dest) {
                Ok(()) => {
                    if let Ok(mut prev) = PREVIOUS_DIR.lock() {
                        *prev = before;
                    }
                    Some(Control::Continue(0))
                }
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
        Ok(status) => status.code().unwrap_or(130),
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
