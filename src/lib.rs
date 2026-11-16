//! The polysh translator as a library.
//!
//! ```
//! use polysh::{translate, Dialect, Translation};
//!
//! match translate("ls -la", Dialect::Cmd) {
//!     Translation::Translated { command, .. } => assert_eq!(command, "dir /A"),
//!     other => panic!("{other:?}"),
//! }
//! ```

pub mod dialect;
mod emit;
mod intent;
mod parse;
pub mod tokenize;
pub mod translate;
pub mod vars;

pub use dialect::Dialect;
pub use translate::{translate, Translation};
