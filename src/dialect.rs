/// A command-line "language" polysh can read and write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Windows `cmd.exe`.
    Cmd,
    /// POSIX `sh` (Linux, macOS, BSD).
    Posix,
}

impl Dialect {
    /// The dialect of the OS polysh is running on.
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
        }
    }
}
