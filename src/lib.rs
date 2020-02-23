use std::ffi::{CStr};
use std::os::raw::{c_int, c_void, c_char};
use std::sync::Once;

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


macro_rules! hook {
    ($call_real: ident, $real_name:expr, ($($names:ident),*), ($($tys:ty),*) -> $ret:ty) => {
        static mut __REAL: Option<fn($($tys),*) -> $ret> = None;
        static __REAL_INITIALIZED: Once = Once::new();

        unsafe fn $call_real($($names : $tys),*) -> $ret {
            __REAL_INITIALIZED.call_once(|| {
                let real_fn = dlsym(RTLD_NEXT, as_char_ptr!($real_name));
                __REAL = Some(std::mem::transmute(real_fn));
            });
            match __REAL {
                None => {
                    panic!("Failed to initialize `{}`", CStr::from_bytes_with_nul_unchecked($real_name).to_string_lossy())
                }
                Some(func) => {
                    func($($names),*)
                }
            }
            
        }
    };
}

hook!(real_open, b"open\0", (path, flags, mode), (*const c_char, c_int, mode_t) -> c_int);

#[no_mangle]
pub unsafe extern fn open(path: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    print!("open({}, {:b}, {:b}) = ", CStr::from_ptr(path).to_string_lossy(), flags, mode);
    let ret = real_open(path, flags, mode);
    println!("{}", ret);
    ret
}
