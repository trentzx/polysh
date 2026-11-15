/// A command-line "language" polysh can read and write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Windows `cmd.exe`.
    Cmd,
    /// POSIX `sh` (Linux, macOS, BSD).
    Posix,
    /// Windows PowerShell / PowerShell 7.
    PowerShell,
}

impl Dialect {
    /// The dialect polysh executes commands in on this OS.
    pub fn native() -> Self {
        if cfg!(windows) {
            Dialect::Cmd
        } else {
            Dialect::Posix
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dialect::Cmd => "Windows cmd",
            Dialect::Posix => "POSIX sh",
            Dialect::PowerShell => "PowerShell",
        }
    }

    /// Parse a user-supplied dialect name (`--to powershell`).
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "cmd" | "windows" | "win" | "bat" => Some(Dialect::Cmd),
            "posix" | "sh" | "bash" | "zsh" | "linux" | "mac" | "macos" | "unix" => {
                Some(Dialect::Posix)
            }
            "powershell" | "pwsh" | "ps" | "ps1" => Some(Dialect::PowerShell),
            _ => None,
        }
    }
}
