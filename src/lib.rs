use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};

// Configuration

#[derive(Debug)]
pub struct Config {
    lower_dir: PathBuf,
    upper_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> Option<Config> {
        let lower_dir = match std::env::var("LIBOVERLAY_LOWER_DIR") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                eprintln!("liboverlay:  LIBOVERLAY_LOWER_DIR not specified");
                return None;
            }
        };

        let upper_dir = match std::env::var("LIBOVERLAY_UPPER_DIR") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                eprintln!("liboverlay:  LIBOVERLAY_UPPER_DIR not specified");
                return None;
            }
        };

        Some(Config {
            lower_dir,
            upper_dir,
        })
    }
}

static mut CONFIG: Option<Config> = None;

#[used]
#[cfg_attr(target_os = "linux", link_section = ".ctors")]
pub static INIT_CONFIG: extern fn() = {
    extern fn init_config_impl() {
        unsafe {
            CONFIG = Config::from_env();
            eprintln!("liboverlay: initialized: {:?}", CONFIG);
        }
    }
    init_config_impl
};

pub fn get_config() -> &'static Option<Config> {
    unsafe { &CONFIG }
}


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

import_real!(C_OPEN, b"open\0", (path: *const c_char, flags: c_int, mode: mode_t) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn open(path: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    print!(
        "open({}, {:b}, {:b}) = ",
        CStr::from_ptr(path).to_string_lossy(),
        flags,
        mode
    );
    let ret = C_OPEN.call(path, flags, mode);
    println!("{}", ret);
    ret
}

import_real!(C_OPEN64, b"open64\0", (path: *const c_char, flags: c_int, mode: mode_t) -> c_int);

#[no_mangle]
pub unsafe extern "C" fn open64(path: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    print!(
        "open64({}, {:b}, {:b}) = ",
        CStr::from_ptr(path).to_string_lossy(),
        flags,
        mode
    );
    let ret = C_OPEN64.call(path, flags, mode);
    println!("{}", ret);
    ret
}
