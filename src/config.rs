use std::path::PathBuf;

#[derive(Debug)]
pub struct Config {
    pub lower_dir: PathBuf,
    pub upper_dir: PathBuf,
    pub debug: bool,
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

        let debug = std::env::var("LIBOVERLAY_DEBUG").map_or(false, |val| &val == "1");

        Some(Config {
            lower_dir,
            upper_dir,
            debug,
        })
    }
}

static mut CONFIG: Option<Config> = None;

#[used]
#[cfg_attr(target_os = "linux", link_section = ".ctors")]
pub static INIT_CONFIG: extern "C" fn() = {
    extern "C" fn init_config_impl() {
        unsafe {
            CONFIG = Config::from_env();
            if let Some(cfg) = CONFIG.as_ref() {
                if cfg.debug {
                    eprintln!("liboverlay: initialized: {:?}", CONFIG);
                }
            }
        }
    }
    init_config_impl
};

#[inline(always)]
pub fn get_config() -> Option<&'static Config> {
    unsafe { CONFIG.as_ref() }
}


#[inline(always)]
pub fn if_debug<F: FnOnce()>(callback: F) {
    if get_config().map_or(false, |cfg| cfg.debug) {
        callback()
    }
}