<div align="center">

# polysh

### Stop translating commands in your head.

Type `ls -la` on Windows or `dir /s` on a Mac, and polysh turns it into the right command for your OS, flags and all.

[![CI](https://github.com/trentzx/polysh/actions/workflows/ci.yml/badge.svg)](https://github.com/trentzx/polysh/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-informational)
![Dependencies](https://img.shields.io/badge/dependencies-0-brightgreen)
![Written in Rust](https://img.shields.io/badge/written%20in-Rust-orange?logo=rust)

<img src="assets/demo.svg" alt="polysh translating Linux commands on Windows and Windows commands on macOS" width="760">

</div>

---

## The problem

You know this one:

```text
C:\> ls -la
'ls' is not recognized as an internal or external command,
operable program or batch file.
```

And the reverse:

```text
$ dir /s
dir: cannot access '/s': No such file or directory
```

If you've ever used more than one OS (a Windows laptop and a Linux server, a Mac at work and a PC at home, or WSL alongside PowerShell), you've typed the wrong dialect, sighed, and looked it up. **polysh fixes that.** Type what your fingers already know, and it runs the right thing.

## Install

**With Cargo** (any OS):

```bash
cargo install --git https://github.com/trentzx/polysh
```

**Prebuilt binaries:** download a single file for Windows, macOS (Intel and Apple Silicon) or Linux from [Releases](https://github.com/trentzx/polysh/releases). Nothing else to install.

## Quick start

```bash
polysh                       # start the interactive shell
polysh -c "rm -rf build"     # translate and run one command
polysh -n "kill -9 4242"     # explain only: show the translation, run nothing
```

`-n` doubles as a cheat sheet. Want to know the Windows way to do something? Ask:

```text
$ polysh -n "grep -rin TODO src"
findstr /I /S /N "TODO" src
```

## What makes it different

- **It translates flags, not just names.** PowerShell aliases `ls` to `Get-ChildItem`, then breaks on `ls -la`. polysh knows `-la` means "all files, long format" and writes `dir /A`.
- **It never guesses.** If it sees a flag it doesn't fully understand (like `rm -i`), it runs your line unchanged rather than doing something you didn't ask for. Destructive commands stay predictable.
- **It shows its work.** Every translation is printed before it runs, so you pick up the other OS's commands just by using it.
- **It's instant and offline.** No AI, no network, no account. A single binary of a few hundred KB with zero dependencies.
- **It stays out of your way.** Commands that are already native run untouched, and so does anything it doesn't recognise, like `git`, `cargo` or `npm`.

## What it translates

| Task | Windows cmd | Linux / macOS |
|---|---|---|
| List files | `dir` `/A` `/S` `/B` `/W` | `ls` `-a` `-l` `-R` |
| Copy | `copy`, `xcopy /E` | `cp`, `cp -r` |
| Move / rename | `move` | `mv` |
| Delete files | `del` / `erase` `/F` `/Q` | `rm` `-r` `-f` |
| Make folder | `mkdir` / `md` | `mkdir -p` |
| Remove folder | `rmdir` / `rd` `/S` `/Q` | `rmdir`, `rm -r` |
| Show a file | `type` | `cat` |
| Clear screen | `cls` | `clear` |
| Network info | `ipconfig` `/all` | `ifconfig`, `ip addr` |
| List processes | `tasklist` | `ps`, `ps aux` |
| Kill by PID | `taskkill /PID n` `/F` | `kill`, `kill -9` |
| Kill by name | `taskkill /IM name.exe` | `pkill`, `killall` |
| Find a program | `where` | `which` |
| Search text | `findstr` `/I` `/S` `/N` | `grep` `-i` `-r` `-n` |

Path separators are converted as well (`dir src\main` becomes `ls -l src/main`). `cd` (including `cd /d`), `pwd`, `help` and `exit` work the same everywhere.

**Missing a command you use every day?** [Request it](https://github.com/trentzx/polysh/issues/new?template=command_request.yml). Each one is a small, self-contained contribution.

## How it works

```text
 "rm -rf build"
       │
       ▼  tokenize        quote-aware, keeps Windows backslashes intact
 [rm] [-rf] [build]
       │
       ▼  parse           detects the dialect from the command and reads its flags
 POSIX · Remove { recursive, force, paths: [build] }
       │
       ▼  emit            writes the same intent in your OS's dialect
 if exist "build\*" (rmdir /S /Q "build") else (del /F /Q "build")
       │
       ▼  run             through cmd /C on Windows, sh -c everywhere else
```

The middle step is the key. Every command is reduced to an **intent** (what it means, not how it's spelled), so adding a dialect means teaching polysh to read and write intents, not writing N×N rules.

## How it compares

| | Knows both dialects | Translates flags | Offline and instant | Nothing new to learn |
|---|:-:|:-:|:-:|:-:|
| **polysh** | ✅ | ✅ | ✅ | ✅ |
| PowerShell aliases | partly | ❌ | ✅ | ✅ |
| WSL / Git Bash / Cygwin | ❌ Linux only | ❌ | ✅ | ✅ |
| Nushell, Xonsh, Fish | ❌ own syntax | ❌ | ✅ | ❌ |
| AI terminals | ✅ | ✅ | ❌ | ✅ |

## Roadmap

- [ ] **Pipes and chains:** translate each part of `ls | grep x` and `cd app && npm i`
- [ ] **PowerShell** as a third dialect (`Get-ChildItem`, `Remove-Item -Recurse`, …)
- [ ] Line editing, history and tab completion
- [ ] Ctrl+C stops the running command instead of exiting polysh
- [ ] Environment variables (`%USERPROFILE%` ↔ `$HOME`)
- [ ] More commands: `ping`, `tree`, `find`, `chmod`/`icacls`, `env`/`set`, `shutdown`
- [ ] Your own rules in a config file
- [ ] `winget`, Homebrew and Scoop packages

## Contributing

polysh is built to be easy to extend: one command is usually about 30 lines and a couple of tests. See [CONTRIBUTING.md](CONTRIBUTING.md) to get started.

## Known limitations

- Pipes, redirection and `&&` are run as typed for now (see the roadmap).
- `type` is treated as the Windows "print file" command, so the POSIX `type <cmd>` built-in isn't available through polysh.
- `cp` and `copy` translate the two-argument form (`src dst`).

## License

[MIT](LICENSE)

<div align="center">

**If polysh saved you a trip to Stack Overflow, a ⭐ helps other people find it.**

</div>
