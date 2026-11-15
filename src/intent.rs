/// What a command *means*, independent of how any one shell spells it.
///
/// Parsers turn a command line into an `Intent`; emitters turn an `Intent`
/// back into a command line for a target dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    ListDir {
        all: bool,
        long: bool,
        recursive: bool,
        paths: Vec<String>,
    },
    Copy {
        recursive: bool,
        src: String,
        dst: String,
    },
    Move {
        src: String,
        dst: String,
    },
    Remove {
        recursive: bool,
        force: bool,
        paths: Vec<String>,
    },
    /// Always creates missing parent folders (cmd's default, POSIX `-p`).
    MakeDir {
        paths: Vec<String>,
    },
    RemoveDir {
        recursive: bool,
        paths: Vec<String>,
    },
    PrintFile {
        paths: Vec<String>,
    },
    Clear,
    NetConfig {
        all: bool,
    },
    ProcessList,
    KillPid {
        force: bool,
        pids: Vec<String>,
    },
    KillName {
        force: bool,
        name: String,
    },
    Which {
        names: Vec<String>,
    },
    Search {
        ignore_case: bool,
        recursive: bool,
        line_numbers: bool,
        pattern: String,
        paths: Vec<String>,
    },
}
