use super::{archive, is_persistent, valid_relative_path, validate_manifest, Manifest};
use crate::interpreter::validate_pack;
use crate::{BasicError, BasicResult, ErrorCode, Interpreter};
use std::collections::BTreeMap;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub struct EmbeddedGame {
    directory: tempfile::TempDir,
    manifest: Manifest,
}

impl EmbeddedGame {
    pub fn open() -> Result<Option<Self>, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        Ok(
            archive::extract(&executable)?.map(|(directory, manifest)| Self {
                directory,
                manifest,
            }),
        )
    }

    pub fn is_gui(&self) -> bool {
        self.manifest.gui
    }

    pub fn prepare(&self, interpreter: &mut Interpreter) -> Result<PathBuf, String> {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or("The Windows LOCALAPPDATA directory is unavailable.")?;
        let save_root = local_app_data.join("AVL BASIC").join(&self.manifest.id);
        let root = self.directory.path();
        interpreter
            .configure_packed_files(
                root.to_path_buf(),
                save_root,
                self.manifest.persistent.clone(),
            )
            .map_err(|error| error.display_diagnostic())?;
        Ok(root.join(&self.manifest.entry))
    }
}

struct Command {
    source: String,
    output: String,
    id: String,
    includes: Vec<String>,
    persistent: Vec<String>,
    gui: bool,
}

pub(crate) fn run_command(
    arguments: &str,
    resolve_input: impl Fn(&str) -> BasicResult<PathBuf>,
    resolve_output: impl Fn(&str) -> BasicResult<PathBuf>,
) -> BasicResult<Vec<String>> {
    let command = Command::parse(arguments)
        .map_err(|error| BasicError::new(ErrorCode::Syntax).with_detail(error))?;
    if !cfg!(target_feature = "crt-static") {
        return Err(BasicError::new(ErrorCode::InvalidArgument)
            .with_detail("PACK requires an AVL BASIC build with the static Windows C runtime."));
    }
    let source = resolve_input(&command.source)?;
    let output = resolve_output(&command.output)?;
    let mut warnings = command
        .build(&source, &output)
        .map_err(|error| BasicError::new(ErrorCode::FileIoError).with_detail(error))?;
    warnings.push(format!("PACK created {}.", command.output));
    Ok(warnings)
}

impl Command {
    fn parse(mut args: &str) -> Result<Self, String> {
        let source = quoted(&mut args)?;
        if !word(&mut args).eq_ignore_ascii_case("TO") {
            return Err("Expected TO followed by the quoted output file name.".into());
        }
        let output = quoted(&mut args)?;
        if !Path::new(&output)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Err("PACK output must end in .exe.".into());
        }

        let mut id = None;
        let mut includes = Vec::new();
        let mut persistent = Vec::new();
        let mut gui = false;
        while !args.trim().is_empty() {
            let option = word(&mut args).to_ascii_uppercase();
            if option == "GUI" {
                if gui {
                    return Err("Duplicate PACK option GUI.".into());
                }
                gui = true;
                continue;
            }
            let value = quoted(&mut args)?;
            match option.as_str() {
                "ID" if id.is_none() => id = Some(value),
                "INCLUDE" => includes.push(value.replace('\\', "/")),
                "PERSIST" => persistent.push(value.replace('\\', "/")),
                "ID" => return Err(format!("Duplicate PACK option {option}.")),
                _ => return Err(format!("Unknown PACK option {option:?}.")),
            }
        }
        let id = id.ok_or("PACK requires a stable game ID.")?;
        Ok(Self {
            source,
            output,
            id,
            includes,
            persistent,
            gui,
        })
    }

    fn build(&self, source: &Path, output: &Path) -> Result<Vec<String>, String> {
        let source = source
            .canonicalize()
            .map_err(|error| format!("Cannot open {:?}: {error}", self.source))?;
        let root = source.parent().ok_or("Missing program directory.")?;
        let entry = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("The BASIC file name must be valid Unicode.")?;
        let manifest = Manifest {
            entry: entry.to_owned(),
            id: self.id.clone(),
            persistent: self.persistent.clone(),
            gui: self.gui,
        };
        validate_manifest(&manifest)?;

        let mut files = BTreeMap::from([(entry.to_owned(), source.clone())]);
        for include in &self.includes {
            let relative = include.strip_suffix('/').unwrap_or(include);
            gather(root, relative, &self.persistent, true, &mut files)?;
        }
        let warnings = validate_pack(root, entry, &mut files, &self.persistent)?;
        let executable = std::env::current_exe()
            .and_then(|path| path.canonicalize())
            .map_err(|error| format!("Cannot locate the PACK runtime: {error}"))?;
        if let Ok(existing) = output.canonicalize() {
            if existing == executable || files.values().any(|path| *path == existing) {
                return Err("PACK output would overwrite one of its input files.".into());
            }
        }
        if files.values().any(|path| *path == executable) {
            return Err("An INCLUDE path contains the PACK runtime itself.".into());
        }
        archive::write(&executable, output, &files, &manifest)?;
        Ok(warnings)
    }
}

fn word<'a>(args: &mut &'a str) -> &'a str {
    let rest = args.trim_start();
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let (word, rest) = rest.split_at(end);
    *args = rest;
    word
}

fn quoted(args: &mut &str) -> Result<String, String> {
    let rest = args
        .trim_start()
        .strip_prefix('"')
        .ok_or("Expected a quoted PACK argument.")?;
    let (value, rest) = rest.split_once('"').ok_or("Unclosed PACK argument.")?;
    if value.is_empty() || (!rest.is_empty() && !rest.starts_with(char::is_whitespace)) {
        return Err(
            "Expected a nonempty quoted PACK argument followed by a space or end of command."
                .into(),
        );
    }
    *args = rest;
    Ok(value.to_owned())
}

fn gather(
    root: &Path,
    relative: &str,
    persistent: &[String],
    explicit: bool,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    if !valid_relative_path(relative) {
        return Err(format!("Invalid INCLUDE path {relative:?}."));
    }
    if !explicit && is_persistent(relative, persistent) {
        return Ok(());
    }

    let path = root.join(relative);
    let metadata = path
        .symlink_metadata()
        .map_err(|error| format!("Cannot include {relative:?}: {error}"))?;
    let resolved = path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve {relative:?}: {error}"))?;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !resolved.starts_with(root)
    {
        return Err(format!("Cannot include linked path {relative:?}."));
    }

    if metadata.is_file() {
        let name = resolved
            .strip_prefix(root)
            .expect("INCLUDE file checked against project root")
            .to_str()
            .ok_or("INCLUDE file name must be valid Unicode.")?
            .replace('\\', "/");
        if !valid_relative_path(&name) {
            return Err(format!("Invalid INCLUDE path {name:?}."));
        }
        files.insert(name, resolved);
    } else if metadata.is_dir() {
        let entries = fs::read_dir(&resolved)
            .map_err(|error| format!("Cannot list INCLUDE folder {relative:?}: {error}"))?;
        for entry in entries {
            let name = entry
                .map_err(|error| format!("Cannot list INCLUDE folder {relative:?}: {error}"))?
                .file_name()
                .into_string()
                .map_err(|_| format!("Non-Unicode file name in INCLUDE folder {relative:?}."))?;
            gather(
                root,
                &format!("{relative}/{name}"),
                persistent,
                false,
                files,
            )?;
        }
    } else {
        return Err(format!(
            "INCLUDE path {relative:?} is not a file or directory."
        ));
    }
    Ok(())
}
