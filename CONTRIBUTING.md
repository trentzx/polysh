# Contributing to polysh

Thanks for helping! The most useful contribution is **a new command translation**. Each one is small, self-contained, and a great first PR.

## Setup

```bash
git clone https://github.com/trentzx/polysh
cd polysh
cargo test
```

## Adding a command

Say you want `ping -c 4 host` ↔ `ping -n 4 host`.

1. **Describe what it means** in [`src/intent.rs`](src/intent.rs):
   ```rust
   Ping { count: Option<String>, host: String },
   ```
2. **Read each spelling** in [`src/parse.rs`](src/parse.rs). Add a match arm for the POSIX form and one for the cmd form in `parse`, and one for the cmdlet in `parse_powershell`. Use `Args::only(...)` (or the `spec` list for `ps_args`) to name the flags you understand, so anything else returns `None`.
3. **Write each spelling** in [`src/emit.rs`](src/emit.rs), with one arm in each of `emit_cmd`, `emit_posix` and `emit_powershell`.
4. **Test both directions** in [`src/translate.rs`](src/translate.rs):
   ```rust
   assert_eq!(tr("ping -c 4 example.com", Cmd), "ping -n 4 example.com");
   assert_eq!(tr("ping -n 4 example.com", Posix), "ping -c 4 example.com");
   ```
5. Add a row to the table in the README.

## Ground rules

- **Never guess.** If a flag's meaning doesn't map cleanly, don't accept it. Returning `None` runs the user's line unchanged, which is always safer than a wrong translation, especially for anything that deletes or kills.
- **Keep dependencies minimal.** The only ones today are `rustyline` (line editing) and `ctrlc`.
- Before opening a PR, run:
  ```bash
  cargo fmt
  cargo clippy --all-targets -- -D warnings
  cargo test
  ```

## Reporting a bad translation

Open a [bug report](https://github.com/trentzx/polysh/issues/new?template=bug_report.yml) with what you typed, what polysh ran (`polysh -n "<command>"` shows it), and what you expected.
