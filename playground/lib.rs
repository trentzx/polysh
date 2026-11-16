//! WebAssembly exports for the playground page.
//!
//! JavaScript calls `alloc(len)`, writes a UTF-8 command line into that
//! memory, then `translate(ptr, len, target)` (which frees it). The result is
//! a UTF-8 string at `result_ptr()` of the returned length:
//! `kind \t detail \t command`, where kind is `native`, `translated` or
//! `untranslated`, and detail is the source dialect or a reason.

use std::cell::RefCell;

use polysh::{Dialect, Translation};

thread_local! {
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let buf = vec![0u8; len].into_boxed_slice();
    Box::into_raw(buf) as *mut u8
}

/// # Safety
/// `ptr`/`len` must come from one `alloc(len)` call and not be used again.
#[no_mangle]
pub unsafe extern "C" fn translate(ptr: *mut u8, len: usize, target: u32) -> usize {
    let bytes = Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len));
    let line = String::from_utf8_lossy(&bytes).into_owned();
    let target = match target {
        0 => Dialect::Cmd,
        1 => Dialect::Posix,
        _ => Dialect::PowerShell,
    };
    let out = match polysh::translate(&line, target) {
        Translation::Native => format!("native\t\t{line}"),
        Translation::Translated { from, command } => {
            format!("translated\t{}\t{command}", from.name())
        }
        Translation::Untranslated(reason) => {
            format!("untranslated\t{}\t{line}", reason.unwrap_or(""))
        }
    };
    RESULT.with(|r| {
        *r.borrow_mut() = out.into_bytes();
        r.borrow().len()
    })
}

#[no_mangle]
pub extern "C" fn result_ptr() -> *const u8 {
    RESULT.with(|r| r.borrow().as_ptr())
}
