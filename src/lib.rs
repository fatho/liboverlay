use std::thread_local;
use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_ushort, c_uchar, c_int, c_void};
use std::path::Path;
use std::sync::Mutex;
use std::collections::{HashMap, HashSet};
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
    let redir_path = with_reentrancy_guard(None, || redirect_path_raw(path, (flags & (O_RDWR | O_WRONLY | O_CREAT)) != 0));
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
    let redir_path = with_reentrancy_guard(None, || redirect_path_raw(path, (flags & (O_RDWR | O_WRONLY | O_CREAT)) != 0));
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
    let redir_path = with_reentrancy_guard(None, || redirect_path_raw(path, (flags & (O_RDWR | O_WRONLY | O_CREAT)) != 0));
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

import_real!(C_STAT, b"__xstat\0", (version: c_int, path: *const c_char, statbuf: *mut c_void) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn __xstat(version: c_int, path: *const c_char, statbuf: *mut c_void) -> c_int {
    eprint!(
        "__xstat({}, {}, {:x}) = ",
        version,
        CStr::from_ptr(path).to_string_lossy(),
        statbuf as usize,
    );
    let redir_path = with_reentrancy_guard(None, || redirect_path_raw(path, false));
    let ret = match redir_path {
        Some(redir) => C_STAT.call(
            version,
            redir.to_bytes_with_nul().as_ptr() as *const c_char,
            statbuf,
        ),
        None => C_STAT.call(version, path, statbuf),
    };
    eprintln!("{}", ret);
    ret
}

import_real!(C_LSTAT, b"__lxstat\0", (version: c_int, path: *const c_char, statbuf: *mut c_void) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn __lxstat(version: c_int, path: *const c_char, statbuf: *mut c_void) -> c_int {
    eprint!(
        "__lxstat({}, {}, {:x}) = ",
        version,
        CStr::from_ptr(path).to_string_lossy(),
        statbuf as usize,
    );
    let redir_path = with_reentrancy_guard(None, || redirect_path_raw(path, false));
    let ret = match redir_path {
        Some(redir) => C_LSTAT.call(
            version,
            redir.to_bytes_with_nul().as_ptr() as *const c_char,
            statbuf,
        ),
        None => C_LSTAT.call(version, path, statbuf),
    };
    eprintln!("{}", ret);
    ret
}


// import_real!(C_FSTATAT, b"__fxstatat\0", (dirfd: c_int, path: *const c_char, statbuf: *mut c_void, flags: c_int) -> c_int);

// #[no_mangle]
// pub unsafe extern "C" fn __fxstatat(dirfd: c_int, path: *const c_char, statbuf: *mut c_void, flags: c_int) -> c_int {
//     eprint!(
//         "__fxstatat({}, {}, {:x}, {}) = ",
//         dirfd,
//         CStr::from_ptr(path).to_string_lossy(),
//         statbuf as usize,
//         flags,
//     );
//     let redir_path = with_reentrancy_guard(None, || redirect_path_raw(path, false));
//     let ret = match redir_path {
//         Some(redir) => C_FSTATAT.call(
//             dirfd,
//             redir.to_bytes_with_nul().as_ptr() as *const c_char,
//             statbuf,
//             flags,
//         ),
//         None => C_FSTATAT.call(dirfd, path, statbuf, flags),
//     };
//     eprintln!("{}", ret);
//     ret
// }


/////////////////////////////////////// Redirection logic ///////////////////////////////////////

fn c_char_ptr_to_path(raw_path: *const c_char) -> &'static Path {
    use std::os::unix::ffi::OsStrExt;

    let cpath = unsafe { CStr::from_ptr(raw_path) };
    let ospath: &std::ffi::OsStr = {
        std::ffi::OsStr::from_bytes(cpath.to_bytes())
    };
    Path::new(ospath)
}

fn redirect_path_raw(raw_path: *const c_char, write: bool) -> Option<CString> {
    use std::os::unix::ffi::OsStrExt;
    let path = c_char_ptr_to_path(raw_path);
    let redirected = redir::redirect_path(path, write)?;

    let credir = CString::new(redirected.as_os_str().as_bytes()).ok()?;
    Some(credir)
}

fn redirect_fopen(raw_path: *const c_char, raw_mode: *const c_char) -> Option<CString> {
    let cmode = unsafe { CStr::from_ptr(raw_mode) };
    redirect_path_raw(raw_path, cmode.to_bytes() != b"r")
}


////////////////////////////////////////////////////////////////////////////


import_real!(C_MKDIR, b"mkdir\0", (path: *const c_char, mode: mode_t) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn mkdir(path: *const c_char, mode: mode_t) -> c_int {
    eprint!(
        "mkdir({}, {:o}) = ",
        CStr::from_ptr(path).to_string_lossy(),
        mode,
    );
    let redir_path = with_reentrancy_guard(None, || redirect_path_raw(path, true));
    let ret = match redir_path {
        Some(redir) => C_MKDIR.call(
            redir.to_bytes_with_nul().as_ptr() as *const c_char,
            mode,
        ),
        None => C_MKDIR.call(path, mode),
    };
    eprintln!("{}", ret);
    ret
}

// TODO: provide view across both upper and lower dir when using opendir etc.

import_real!(C_OPENDIR, b"opendir\0", (path: *const c_char, mode: mode_t) -> *mut c_void);

#[no_mangle]
pub unsafe extern "C" fn opendir(path: *const c_char, mode: mode_t) -> *mut c_void {
    eprint!(
        "opendir({}, {:o}) = ",
        CStr::from_ptr(path).to_string_lossy(),
        mode,
    );
    let redir_path = with_reentrancy_guard(None, || redirect_path_raw(path, false));
    let ret = match redir_path {
        Some(redir) => {
            let lower_dir = C_OPENDIR.call(path, mode);

            let upper_dir = C_OPENDIR.call(
                redir.to_bytes_with_nul().as_ptr() as *const c_char,
                mode,
            );

            if ! lower_dir.is_null() {
                eprintln!("liboverlayf: merging opendir");
                // If the lower dir exists, we need to merge the contents of the two dirs
                let mut opendirs = opendirs().lock().unwrap();
                
                let opendir = OpenDir {
                    upper: upper_dir,
                    lower: lower_dir,
                    seen: HashSet::new(),
                };
                opendirs.insert(upper_dir as usize, opendir);
            }
            upper_dir
        }
        None => C_OPENDIR.call(path, mode),
    };
    eprintln!("{:x}", ret as usize);
    ret
}

pub type ino_t = u64;
pub type off_t = i64;
#[repr(C)]
pub struct dirent {
    pub d_ino: ino_t,
    pub d_off: off_t,
    pub d_reclen: c_ushort,
    pub d_type: c_uchar,
    pub d_name: [c_char; 256],
}

import_real!(C_READDIR, b"readdir\0", (dir: *mut c_void) -> *mut dirent);

#[no_mangle]
pub unsafe extern "C" fn readdir(dir: *mut c_void) -> *mut dirent {
    eprint!(
        "readdir({:x}) = ",
        dir as usize,
    );
    let ret = IS_HOOKED.with(|is_hooked: &Cell<bool>| {
        if is_hooked.get() {
            C_READDIR.call(dir)
        } else {
            let mut opendirs = opendirs().lock().unwrap();
            if let Some(merged) = opendirs.get_mut(&(dir as usize)) {
                // First try upper
                let entry: *mut dirent = C_READDIR.call(dir);
                if entry.is_null() {
                    // Now try lower
                    loop {
                        let entry_lower = C_READDIR.call(merged.lower);
                        if entry_lower.is_null() {
                            break entry_lower
                        } else {
                            // filter out entries from top level
                            let name = CStr::from_ptr(&std::ptr::read(entry_lower).d_name[0] as *const i8);
                            if ! merged.seen.contains(name) {
                                break entry_lower
                            }
                        }
                    }
                } else {
                    // remember name
                    let name = CStr::from_ptr(&std::ptr::read(entry).d_name[0] as *const i8);
                    merged.seen.insert(name.to_owned());
                    entry
                }
            } else {
                C_READDIR.call(dir)
            }            
        }
    });
    eprintln!("{:x}", ret as usize);
    ret
}


import_real!(C_CLOSEDIR, b"closedir\0", (dir: *mut c_void) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn closedir(dir: *mut c_void) -> c_int {
    eprint!(
        "closedir({:x}) = ",
        dir as usize,
    );
    with_reentrancy_guard((), || {
        let removed = opendirs().lock().unwrap().remove(&(dir as usize));
        if let Some(od) = removed {
            // Only close lower dir as the upper dir is used as key and will be closed down below
            eprintln!("liboverlay: closing merged opendir");
            C_CLOSEDIR.call(od.lower);
        }
    });
    let ret = C_CLOSEDIR.call(dir);
    eprintln!("{}", ret);
    ret
}


static mut OPENDIRS: Option<Mutex<HashMap<usize, OpenDir>>> = None;

#[used]
#[cfg_attr(target_os = "linux", link_section = ".ctors")]
pub static INIT_OPENDIRS: extern "C" fn() = {
    extern "C" fn init() {
        unsafe {
            OPENDIRS = Some(Mutex::new(HashMap::new()));
        }
    }
    init
};

fn opendirs() -> &'static Mutex<HashMap<usize, OpenDir>> {
    unsafe { OPENDIRS.as_ref().unwrap() }
}

#[derive(Clone)]
struct OpenDir {
    upper: *mut c_void,
    lower: *mut c_void,
    seen: HashSet<CString>,
}

unsafe impl Send for OpenDir {}
unsafe impl Sync for OpenDir {}