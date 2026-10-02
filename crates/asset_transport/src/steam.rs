#[cfg(windows)]
use std::collections::HashSet;
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;

use crate::discover::GamesRoot;
#[cfg(windows)]
use crate::discover::{IW4_ZONE_VERSION, peek_zone_version, search_roots};

pub const MW2_SHORTCUT: &str = "Modern Warfare 2.lnk";

#[derive(Clone, Debug, Default)]
pub struct SteamProbe {
    pub steam_found: bool,
    pub tried: Vec<(PathBuf, SteamCandidate)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SteamCandidate {
    Missing,
    NoMultiplayerData,
    Linked,
    OneOfSeveral,
    ShortcutFailed(String),
}

#[cfg(windows)]
pub fn link_steam_mw2(root: &GamesRoot) -> SteamProbe {
    let mut probe = SteamProbe::default();
    let link = root.0.join(MW2_SHORTCUT);
    if link.exists() || has_mw2(&search_roots(&root.0)) {
        return probe;
    }
    let libraries = steam_libraries();
    probe.steam_found = !libraries.is_empty();
    let mut valid = Vec::new();
    for library in libraries {
        let path = library
            .join("steamapps")
            .join("common")
            .join("Call of Duty Modern Warfare 2");
        let candidate = if !path.is_dir() {
            SteamCandidate::Missing
        } else if has_mw2(std::slice::from_ref(&path)) {
            valid.push(probe.tried.len());
            SteamCandidate::Linked
        } else {
            SteamCandidate::NoMultiplayerData
        };
        probe.tried.push((path, candidate));
    }
    match valid[..] {
        [index] => {
            let (target, candidate) = &mut probe.tried[index];
            match create_shortcut(&link, target) {
                Ok(()) => diag::info!(
                    Zone,
                    "linked Steam MW2 {} as {}",
                    target.display(),
                    link.display()
                ),
                Err(error) => *candidate = SteamCandidate::ShortcutFailed(error),
            }
        }
        [] => {}
        _ => {
            for index in valid {
                probe.tried[index].1 = SteamCandidate::OneOfSeveral;
            }
        }
    }
    probe
}

#[cfg(not(windows))]
pub fn link_steam_mw2(_root: &GamesRoot) -> SteamProbe {
    SteamProbe::default()
}

#[cfg(windows)]
fn has_mw2(roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        std::iter::once(root.clone())
            .chain(subdirs(root))
            .flat_map(|tree| subdirs(&tree.join("zone")))
            .any(|language| {
                peek_zone_version(&language.join("common_mp.ff")) == Some(IW4_ZONE_VERSION)
            })
    })
}

#[cfg(windows)]
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                diag::warn!(Zone, "game probe {}: {error}", dir.display());
            }
            Vec::new()
        }
    }
}

#[cfg(windows)]
fn create_shortcut(link: &Path, target: &Path) -> Result<(), String> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};
    // SAFETY: COM is initialized on this thread for the duration of the calls and
    // uninitialized only when this call initialized it.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| -> windows::core::Result<()> {
            let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            shell_link.SetPath(&HSTRING::from(target.as_os_str()))?;
            shell_link
                .cast::<IPersistFile>()?
                .Save(&HSTRING::from(link.as_os_str()), true)
        })();
        if initialized {
            CoUninitialize();
        }
        result.map_err(|error| format!("cannot create {}: {error}", link.display()))
    }
}

#[cfg(windows)]
fn steam_libraries() -> Vec<PathBuf> {
    let mut installs = Vec::new();
    for install in [
        registry_string(true, "Software\\Valve\\Steam", "SteamPath"),
        registry_string(false, "SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath"),
        Some(PathBuf::from("C:\\Program Files (x86)\\Steam")),
        Some(PathBuf::from("C:\\Steam")),
    ]
    .into_iter()
    .flatten()
    {
        let install = PathBuf::from(install.to_string_lossy().replace('/', "\\"));
        if install.is_dir() && !installs.contains(&install) {
            installs.push(install);
        }
    }
    let mut libraries: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();
    for install in &installs {
        let vdf = install.join("steamapps").join("libraryfolders.vdf");
        let listed = match std::fs::read_to_string(&vdf) {
            Ok(text) => steam_library_paths(&text),
            Err(error) => {
                diag::warn!(Zone, "steam libraries {}: {error}", vdf.display());
                Vec::new()
            }
        };
        for library in std::iter::once(install.clone()).chain(listed) {
            if let Ok(canonical) = std::fs::canonicalize(&library)
                && seen.insert(canonical)
            {
                libraries.push(library);
            }
        }
    }
    libraries
}

#[cfg(windows)]
fn steam_library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| line.trim().strip_prefix("\"path\""))
        .filter_map(|rest| {
            let value = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
            Some(PathBuf::from(value.replace("\\\\", "\\")))
        })
        .collect()
}

#[cfg(windows)]
fn registry_string(current_user: bool, key: &str, value: &str) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
    };
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, value) = (wide(key), wide(value));
    let hive = if current_user {
        HKEY_CURRENT_USER
    } else {
        HKEY_LOCAL_MACHINE
    };
    let mut buffer = [0u16; 1024];
    let mut bytes = (buffer.len() * 2) as u32;
    // SAFETY: both names are NUL-terminated and `bytes` is the buffer's size in bytes.
    let status = unsafe {
        RegGetValueW(
            hive,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != 0 {
        return None;
    }
    let len = (bytes as usize / 2).saturating_sub(1);
    Some(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..len])))
}
