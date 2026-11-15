# polysh

**One shell, every dialect.** Type the commands you already know — Windows `cmd` or Linux/macOS — and polysh translates them, flags included, to whatever OS you're actually on.

```text
polysh C:\Users\me\project> ls -la
  ↳ dir /A   (POSIX sh → Windows cmd)
 Volume in drive C has no label.
 ...

polysh /home/me/project> dir /s /b
  ↳ ls -R   (Windows cmd → POSIX sh)
 ...
```

## Why

Switching between Windows, Linux, and macOS means constantly hitting this:

```text
C:\> ls -la
'ls' is not recognized as an internal or external command
```

Existing tools only solve half of it:

| Tool | Gap |
|---|---|
| WSL / Git Bash / Cygwin | Give you Linux commands on Windows, but your `dir` / `del` / `copy` habits stop working. |
| PowerShell aliases | `ls`, `cp`, `rm` exist as **names**, but Linux **flags** (`ls -la`, `rm -rf`) break. |
| Nushell, Xonsh, etc. | Cross-platform, but each is a *new* syntax to learn. |
| AI terminals | Need a network round-trip and a model; not instant or deterministic. |

polysh is the missing piece: a small, fast, offline, deterministic translator that understands **both** dialects and shows you exactly what it ran.

## How it works

```text
 your input ──► tokenize ──► parse (detect dialect) ──► Intent ──► emit (native dialect) ──► run
   "rm -rf build"            POSIX `rm`, flags r+f      Remove{recursive, force}     rmdir /S /Q ...
```

1. **Tokenize** the line (quote-aware).
2. **Parse** the command name to detect its dialect (`dir` → cmd, `ls` → POSIX) and read its flags (`/S` vs `-R`) into a dialect-neutral **Intent**.
3. **Emit** that intent in your OS's native dialect.
4. **Run** it through the system shell (`cmd /C` on Windows, `sh -c` elsewhere), printing the translation first so you learn as you go.

Safety rules:

- If a command is **already native**, it runs untouched.
- If polysh sees a flag it doesn't fully understand (e.g. `rm -i`), it **does not guess**; it passes the line through unchanged.
- Pipes and redirection (`|`, `>`, `&&`, `;`) are passed through as-is for now.

## Supported commands (v0.1)

| Task | Windows cmd | Linux / macOS |
|---|---|---|
| List files | `dir` `/A` `/S` `/B` `/W` | `ls` `-a` `-l` `-R` |
| Copy | `copy`, `xcopy /E` | `cp`, `cp -r` |
| Move / rename | `move` | `mv` |
| Delete files | `del` / `erase` `/F` `/Q` | `rm` `-r` `-f` |
| Make folder | `mkdir` / `md` | `mkdir -p` |
| Remove folder | `rmdir` / `rd` `/S` `/Q` | `rmdir`, `rm -r` |
| Show file | `type` | `cat` |
| Clear screen | `cls` | `clear` |
| Network info | `ipconfig` `/all` | `ifconfig`, `ip addr` |
| List processes | `tasklist` | `ps`, `ps aux` |
| Kill by PID | `taskkill /PID n` `/F` | `kill`, `kill -9` |
| Kill by name | `taskkill /IM name.exe` | `pkill`, `killall` |
| Find a program | `where` | `which` |
| Search text | `findstr` `/I` `/S` `/N` | `grep` `-i` `-r` `-n` |

Built-ins that work everywhere: `cd` / `chdir` (incl. `cd /d`), `pwd`, `help`, `exit`.

Path separators are converted too: `dir src\main` → `ls -l src/main`.

## Install

Requires [Rust](https://rustup.rs) (stable).

```bash
git clone <this-repo-url> polysh
cd polysh
cargo install --path .
```

Or build without installing:

```bash
cargo build --release
```

The binary lands in `target/release/polysh` (`polysh.exe` on Windows). It's a single file with no runtime dependencies.

## Usage

```bash
polysh                      # interactive shell
polysh -c "ls -la"          # translate and run one command
polysh -n "rm -rf build"    # explain only: print the translation, run nothing
polysh --help
```

## Project layout

```text
src/
├── main.rs       CLI entry point, REPL, built-ins, process execution
├── dialect.rs    Dialect enum (Windows cmd, POSIX sh) and native detection
├── tokenize.rs   Quote-aware tokenizer and operator detection
├── intent.rs     Dialect-neutral description of what a command does
├── parse.rs      Command line (any dialect) → Intent
├── emit.rs       Intent → command line in a target dialect
└── translate.rs  Glue: line in, Translation out (+ tests)
```

Adding a command means: add an `Intent` variant, a parser arm for each dialect's spelling, and an emitter arm for each target, plus tests.

## Roadmap

- [ ] PowerShell as a third dialect (`Get-ChildItem`, `Remove-Item -Recurse`, …)
- [ ] Translate each segment of pipelines and `&&` chains
- [ ] Line editing, history, and tab completion
- [ ] Ctrl+C forwards to the running command instead of exiting polysh
- [ ] Environment variable syntax (`%VAR%` ↔ `$VAR`)
- [ ] More commands: `ping`, `tree`, `find`, `chmod`/`icacls`, `env`/`set`, `shutdown`
- [ ] User-defined translation rules in a config file
- [ ] Optional AI fallback for commands with no rule

## Known limitations

- `type` is treated as the Windows "print file" command, so the POSIX `type <cmd>` built-in isn't available through polysh.
- `cp`/`copy` translate only the two-argument form (`src dst`).
- Translations aim for the common meaning of each command; edge-case flags pass through untranslated rather than being guessed.

## License

MIT. See [LICENSE](LICENSE).
