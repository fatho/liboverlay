use std::path::{Path, PathBuf};

use crate::config;

pub fn redirect_path(path: &Path, write: bool) -> Option<PathBuf> {
    if path.is_relative() {
        config::if_debug(|| eprintln!("liboverlay: relative paths not supported {}", path.display()));
        return None;
    }
    // TODO: do things break when path contains `..` in the middle?

    let cfg = config::get_config()?;
    // Only redirect accesses to the lower directory, ignore any other accesses
    let path_in_lower = path.strip_prefix(&cfg.lower_dir).ok()?;

    let path_to_upper = cfg.upper_dir.join(path_in_lower);

    // If the path alrady exists in the upper directory, redirect to that one
    let redirect = if path_to_upper.exists() {
        true
    // If the flags imply write access, make a copy and redirect to that one
    } else if write {
        let parent_in_lower = path.parent()?;

        if parent_in_lower.exists() {
            // Make sure the directory exists
            let parent_in_upper = path_to_upper.parent()?;
            std::fs::create_dir_all(parent_in_upper)
                .map_err(|e| {
                    config::if_debug(|| eprintln!(
                        "liboverlay: could not create {}: {}",
                        parent_in_upper.display(),
                        e
                    ))
                })
                .ok()?;

            // Copy source file if it exists
            if path.is_file() {
                config::if_debug(|| eprintln!("liboverlay: making writable copy"));
                // HACK: This relies crucially on the fact that fs::copy first opens the source path,
                //  otherwise, our own redirection logic would apply and send the read request to the
                //  newly created upper file.
                // HACK: This is not thread safe!
                std::fs::copy(path, &path_to_upper)
                    .map_err(|e| {
                        config::if_debug(|| eprintln!(
                            "liboverlay: failed to copy from lower {} to upper {}: {}",
                            path.display(),
                            path_to_upper.display(),
                            e
                        ))
                    })
                    .ok()?;
                let mut perms = std::fs::metadata(&path_to_upper).ok()?.permissions();
                perms.set_readonly(false);
                std::fs::set_permissions(&path_to_upper, perms).ok()?;
            }
        }
        true
    } else {
        false
    };

    if redirect {
        config::if_debug(|| eprintln!(
            "liboverlay: redirecting {} to {}",
            path.display(),
            path_to_upper.display()
        ));
        Some(path_to_upper)
    } else {
        None
    }
}
