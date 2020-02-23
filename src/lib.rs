use std::thread_local;
use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;
use std::sync::atomic::{AtomicPtr, Ordering};

mod redir;
mod config;

/////////////////////////////////////// Symbol lookup/redirection ///////////////////////////////////////

macro_rules! as_char_ptr {
    ($bytes: expr) => {
        $bytes as *const u8 as *const c_char
    };
}

extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_NEXT: *mut c_void = -1 as isize as usize as *mut c_void;

#[allow(non_camel_case_types)]
type mode_t = c_int;

macro_rules! import_real {
    ($call_real: ident, $real_name:expr, ($($names:ident : $tys:ty),*) -> $ret:ty) => {
        #[allow(non_camel_case_types)]
        struct $call_real {
            real: AtomicPtr<c_void>,
        }

        impl $call_real {
            unsafe fn call(&self, $($names : $tys),*) -> $ret {
                let mut real_fn = self.real.load(Ordering::SeqCst);
                if real_fn.is_null() {
                    real_fn = dlsym(RTLD_NEXT, as_char_ptr!($real_name));
                    if real_fn.is_null() {
                        panic!("Could not locate real symbol `{}`", CStr::from_bytes_with_nul_unchecked($real_name).to_string_lossy());
                    } else {
                        self.real.store(real_fn, Ordering::SeqCst)
                    }
                }
                let func: extern fn($($tys),*) -> $ret = std::mem::transmute(real_fn);
                func($($names),*)
            }
        }

        static $call_real: $call_real = $call_real {
            real: AtomicPtr::new(std::ptr::null_mut())
        };
    };
}


/////////////////////////////////////// Actual hooks ///////////////////////////////////////

const O_WRONLY: c_int = 01;
const O_RDWR: c_int = 02;
const O_CREAT: c_int = 0x0200;

// Skip hooks while executing a hook
thread_local! {
    static IS_HOOKED: Cell<bool> = Cell::new(false);
}

fn with_reentrancy_guard<R, F: FnOnce() -> R>(default_: R, call: F) -> R {
    IS_HOOKED.with(|is_hooked: &Cell<bool>| {
         if is_hooked.get() {
             default_
         } else {
             is_hooked.set(true);
             let ret = call();
             is_hooked.set(false);
             ret
         }
    })
}

// HACK: open is actually a varargs function, and `mode` only has to be passed
// when flags contains O_CREAT. It seems to work anyway...
import_real!(C_OPEN, b"open\0", (path: *const c_char, flags: c_int, mode: mode_t) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn open(path: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    eprint!(
        "open({}, {:b}, {:b}) = ",
        CStr::from_ptr(path).to_string_lossy(),
        flags,
        mode
    );
    let redir_path = with_reentrancy_guard(None, || redirect_open(path, flags));
    let ret = match redir_path {
        Some(redir) => C_OPEN.call(
            redir.to_bytes_with_nul().as_ptr() as *const c_char,
            flags,
            mode,
        ),
        None => C_OPEN.call(path, flags, mode),
    };
    eprintln!("{}", ret);
    ret
}

import_real!(C_OPEN64, b"open64\0", (path: *const c_char, flags: c_int, mode: mode_t) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn open64(path: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    eprint!(
        "open64({}, {:b}, {:b}) = ",
        CStr::from_ptr(path).to_string_lossy(),
        flags,
        mode
    );
    let redir_path = with_reentrancy_guard(None, || redirect_open(path, flags));
    let ret = match redir_path {
        Some(redir) => C_OPEN64.call(
            redir.to_bytes_with_nul().as_ptr() as *const c_char,
            flags,
            mode,
        ),
        None => C_OPEN64.call(path, flags, mode),
    };
    eprintln!("{}", ret);
    ret
}

import_real!(C_OPENAT, b"openat\0", (dirfd: c_int, path: *const c_char, flags: c_int, mode: mode_t) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn openat(dirfd: c_int, path: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    eprint!(
        "openat({}, {}, {:b}, {:b}) = ",
        dirfd,
        CStr::from_ptr(path).to_string_lossy(),
        flags,
        mode
    );
    // When path is absolute, dirfd will be ignored.
    let redir_path = with_reentrancy_guard(None, || redirect_open(path, flags));
    let ret = match redir_path {
        Some(redir) => C_OPENAT.call(
            dirfd,
            redir.to_bytes_with_nul().as_ptr() as *const c_char,
            flags,
            mode,
        ),
        None => C_OPENAT.call(dirfd, path, flags, mode),
    };
    eprintln!("{}", ret);
    ret
}

import_real!(C_FOPEN, b"fopen\0", (path: *const c_char, mode: *const c_char) -> *mut c_void);

#[no_mangle]
pub unsafe extern "C" fn fopen(path: *const c_char, mode: *const c_char) -> *mut c_void {
    eprint!(
        "fopen({}, {}) = ",
        CStr::from_ptr(path).to_string_lossy(),
        CStr::from_ptr(mode).to_string_lossy(),
    );
    let redir_path = with_reentrancy_guard(None, || redirect_fopen(path, mode));
    let ret = match redir_path {
        Some(redir) => C_FOPEN.call(
            redir.to_bytes_with_nul().as_ptr() as *const c_char,
            mode,
        ),
        None => C_FOPEN.call(path, mode),
    };
    eprintln!("{:x}", ret as usize);
    ret
}

/////////////////////////////////////// Redirection logic ///////////////////////////////////////

fn redirect_open(raw_path: *const c_char, flags: c_int) -> Option<CString> {
    use std::os::unix::ffi::OsStrExt;

    let cpath = unsafe { CStr::from_ptr(raw_path) };
    let ospath: &std::ffi::OsStr = {
        std::ffi::OsStr::from_bytes(cpath.to_bytes())
    };
    let path = Path::new(ospath);
    let redirected = redir::redirect_path(path, (flags & (O_WRONLY | O_RDWR | O_CREAT)) != 0)?;

    let credir = CString::new(redirected.as_os_str().as_bytes()).ok()?;
    Some(credir)
}

fn redirect_fopen(raw_path: *const c_char, raw_mode: *const c_char) -> Option<CString> {
    use std::os::unix::ffi::OsStrExt;
    
    let cpath = unsafe { CStr::from_ptr(raw_path) };
    let ospath: &std::ffi::OsStr = {
        std::ffi::OsStr::from_bytes(cpath.to_bytes())
    };
    let path = Path::new(ospath);

    let cmode = unsafe { CStr::from_ptr(raw_mode) };

    let redirected = redir::redirect_path(path, cmode.to_bytes() != b"r")?;

    let credir = CString::new(redirected.as_os_str().as_bytes()).ok()?;
    Some(credir)
}
