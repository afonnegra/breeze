//! Launcher-independent Breeze data root resolution (TD-010).
//!
//! Under an Explorer double-click, `dirs::data_dir()` was observed to
//! resolve differently (or to `None`) than under a terminal launch, so
//! neither the log directory nor the config file landed where the app
//! looked. The `%APPDATA%` environment variable, by contrast, is set by
//! Windows for every process in the user session regardless of launcher,
//! so it is tried first; the Known Folder API and an exe-relative folder
//! are conservative fallbacks. Config and logs MUST resolve their
//! `Breeze` root through this one chain so they never disagree.

use std::path::{Path, PathBuf};

/// Name of the per-user data folder shared by config and logs.
const BREEZE_DIR: &str = "Breeze";

/// Absolute `Breeze` data root, resolved launcher-independently.
///
/// Thin wrapper over the pure [`resolve_breeze_root`] that reads the
/// three candidate roots from the live process environment.
pub fn breeze_root() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    resolve_breeze_root(
        std::env::var_os("APPDATA").map(PathBuf::from),
        dirs::data_dir(),
        exe_dir,
    )
}

/// Pure resolution of the `Breeze` data root from its three candidate
/// roots, in priority order: `%APPDATA%` env var, then the Known Folder
/// data dir, then an exe-relative folder, then the current directory as
/// a last resort. Split out so the launcher-independence guarantee
/// (TD-010) is unit-testable without touching the process environment.
pub fn resolve_breeze_root(
    appdata: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    exe_dir: Option<PathBuf>,
) -> PathBuf {
    if let Some(appdata) = appdata {
        return appdata.join(BREEZE_DIR);
    }
    if let Some(data_dir) = data_dir {
        return data_dir.join(BREEZE_DIR);
    }
    if let Some(exe_dir) = exe_dir {
        return exe_dir;
    }
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_appdata_env_var() {
        let got = resolve_breeze_root(
            Some(PathBuf::from("R:/Roaming")),
            Some(PathBuf::from("D:/Known")),
            Some(PathBuf::from("E:/app")),
        );
        assert_eq!(got, PathBuf::from("R:/Roaming").join("Breeze"));
    }

    #[test]
    fn falls_back_to_known_folder_when_no_appdata() {
        let got = resolve_breeze_root(
            None,
            Some(PathBuf::from("D:/Known")),
            Some(PathBuf::from("E:/app")),
        );
        assert_eq!(got, PathBuf::from("D:/Known").join("Breeze"));
    }

    #[test]
    fn falls_back_to_exe_dir_when_no_known_folder() {
        let got = resolve_breeze_root(None, None, Some(PathBuf::from("E:/app")));
        assert_eq!(got, PathBuf::from("E:/app"));
    }

    #[test]
    fn falls_back_to_cwd_as_last_resort() {
        let got = resolve_breeze_root(None, None, None);
        assert_eq!(got, PathBuf::from("."));
    }
}
