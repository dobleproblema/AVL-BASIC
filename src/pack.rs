#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod archive;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod command;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(crate) use command::run_command;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub use command::EmbeddedGame;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    entry: String,
    id: String,
    persistent: Vec<String>,
    #[serde(default)]
    gui: bool,
}

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
pub(crate) fn run_command(
    _arguments: &str,
    _resolve_input: impl Fn(&str) -> crate::BasicResult<std::path::PathBuf>,
    _resolve_output: impl Fn(&str) -> crate::BasicResult<std::path::PathBuf>,
) -> crate::BasicResult<Vec<String>> {
    Err(crate::BasicError::new(crate::ErrorCode::InvalidArgument)
        .with_detail("PACK is available on Windows x64."))
}

pub(crate) fn valid_relative_path(path: &str) -> bool {
    path.split('/').all(|part| {
        if part.is_empty()
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|ch| ch.is_control() || "\\:<>\"|?*".contains(ch))
        {
            return false;
        }
        let stem = part.split('.').next().unwrap_or(part).trim_end_matches(' ');
        let stem = stem.to_ascii_uppercase();
        !matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) && !stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
    })
}

pub(crate) fn is_persistent(path: &str, persistent: &[String]) -> bool {
    persistent.iter().any(|item| match item.strip_suffix('/') {
        Some(directory) => {
            path.as_bytes().get(directory.len()) == Some(&b'/')
                && path[..directory.len()].eq_ignore_ascii_case(directory)
        }
        None => path.eq_ignore_ascii_case(item),
    })
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn valid_game_id(id: &str) -> bool {
    id.len() <= 80
        && id.starts_with(|ch: char| ch.is_ascii_alphanumeric())
        && id
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || b".-_".contains(&ch))
        && valid_relative_path(id)
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    use std::path::Path;

    if !valid_game_id(&manifest.id) {
        return Err(format!("Invalid PACK game ID {:?}.", manifest.id));
    }
    if !valid_relative_path(&manifest.entry)
        || !Path::new(&manifest.entry)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bas"))
    {
        return Err(format!("Invalid PACK entry program {:?}.", manifest.entry));
    }
    for item in &manifest.persistent {
        if !valid_relative_path(item.strip_suffix('/').unwrap_or(item)) {
            return Err(format!("Invalid PERSIST path {item:?}."));
        }
    }
    if is_persistent(&manifest.entry, &manifest.persistent) {
        return Err("The entry program cannot be declared persistent.".into());
    }
    Ok(())
}
