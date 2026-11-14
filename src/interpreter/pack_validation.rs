use super::{
    compile_expression, extract_quoted_path_text, file_syntax_separator, first_word_is,
    has_windows_drive_prefix, import_argument, read_program_file, split_arguments, split_commands,
    split_else, split_if_condition_and_rest, starts_keyword, ActiveModule, Expr, ImportDeclaration,
    Interpreter,
};
use crate::pack::{is_persistent, valid_relative_path};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn validate_pack(
    root: &Path,
    entry: &str,
    files: &mut BTreeMap<String, PathBuf>,
    persistent: &[String],
) -> Result<Vec<String>, String> {
    let mut checker = Checker {
        root,
        files,
        persistent,
        pending: vec![Source::new(entry.to_string(), SourceKind::Program)],
        seen: HashSet::new(),
        warnings: Vec::new(),
    };
    while let Some(source) = checker.pending.pop() {
        if checker.seen.insert(source.clone()) {
            checker.scan(&source)?;
        }
    }
    Ok(checker.warnings)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SourceKind {
    Program,
    Library,
    Merge,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Source {
    name: String,
    base: String,
    kind: SourceKind,
}

impl Source {
    fn new(name: String, kind: SourceKind) -> Self {
        let base = name.rsplit_once('/').map_or("", |(parent, _)| parent);
        Self {
            base: base.to_string(),
            name,
            kind,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    DataRead,
    DataWrite,
    ImageRead,
    ImageWrite,
    Import,
    Chain,
    Merge,
}

impl Access {
    fn is_program(self) -> bool {
        matches!(self, Self::Import | Self::Chain | Self::Merge)
    }
}

struct Checker<'a> {
    root: &'a Path,
    files: &'a mut BTreeMap<String, PathBuf>,
    persistent: &'a [String],
    pending: Vec<Source>,
    seen: HashSet<Source>,
    warnings: Vec<String>,
}

impl Checker<'_> {
    fn scan(&mut self, source: &Source) -> Result<(), String> {
        let path = self
            .files
            .get(&source.name)
            .ok_or_else(|| format!("{} is missing from the package", source.name))?;
        let program = read_program_file(path)
            .map_err(|error| format!("{}: {}", source.name, error.display_diagnostic()))?;
        let mut interpreter = Interpreter::new();
        interpreter.program = program;

        // MERGE reads a fragment in its caller's context. The runtime only
        // validates its routine catalog after it combines the two programs.
        if source.kind != SourceKind::Merge {
            if source.kind == SourceKind::Library {
                interpreter.active_module = Some(ActiveModule {
                    path: path.clone(),
                    function_base: 0,
                    sub_base: 0,
                    gosub_depth: 0,
                });
            }
            interpreter.rebuild_command_cache();
            let imports = interpreter.collect_program_routines().map_err(|error| {
                format!(
                    "{}: {}",
                    source.name,
                    interpreter.with_current_line(error).display_diagnostic()
                )
            })?;
            for import in imports {
                self.check(source, import.line, "IMPORT", Access::Import, &import.path)?;
            }
        }

        for line in interpreter.program.line_numbers() {
            for command in split_commands(interpreter.program.get(line).unwrap_or("")) {
                self.inspect(source, line, &command)?;
            }
        }
        Ok(())
    }

    fn inspect(&mut self, source: &Source, line: i32, command: &str) -> Result<(), String> {
        if first_word_is(command, "IF") {
            if let Some((_, rest)) =
                split_if_condition_and_rest(command, &command.to_ascii_uppercase())
            {
                let (then_part, else_part) = split_else(rest);
                for branch in std::iter::once(then_part).chain(else_part) {
                    for nested in split_commands(branch) {
                        self.inspect(source, line, &nested)?;
                    }
                }
            }
            return Ok(());
        }

        if let Some(argument) = import_argument(command) {
            if source.kind == SourceKind::Merge {
                let import = ImportDeclaration::parse(line, argument)
                    .map_err(|error| format!("{}: {}", source.name, error.display_diagnostic()))?;
                self.check(source, line, "IMPORT", Access::Import, &import.path)?;
            }
            return Ok(());
        }

        let Some((operation, access, expression)) = file_expression(command) else {
            return Ok(());
        };
        let context = format!("{}:{line} {operation}", source.name);
        let path = if access.is_program() && expression.starts_with('"') {
            Some(
                extract_quoted_path_text(&expression)
                    .map_err(|error| format!("{context}: {}", error.display_diagnostic()))?,
            )
        } else {
            match compile_expression(&expression)
                .map_err(|error| format!("{context}: {}", error.display_diagnostic()))?
            {
                Expr::Str(path) => Some(path),
                _ => None,
            }
        };
        if let Some(path) = path {
            self.check(source, line, operation, access, &path)
        } else {
            self.warnings.push(format!(
                "{context}: cannot verify the calculated path {expression}"
            ));
            Ok(())
        }
    }

    fn check(
        &mut self,
        source: &Source,
        line: i32,
        operation: &str,
        access: Access,
        path: &str,
    ) -> Result<(), String> {
        let context = format!("{}:{line} {operation}", source.name);
        let relative = relative_path(&source.base, path, access)
            .ok_or_else(|| format!("{context}: invalid package path {path:?}"))?;
        let is_persistent = is_persistent(&relative, self.persistent);
        if matches!(access, Access::DataWrite | Access::ImageWrite) {
            return if is_persistent {
                Ok(())
            } else {
                Err(format!(
                    "{context}: declare {relative:?} with PERSIST to write it"
                ))
            };
        }
        if is_persistent {
            return if access.is_program() {
                Err(format!(
                    "{context}: program file {relative:?} cannot be persistent"
                ))
            } else {
                Ok(())
            };
        }

        let included = self.files.get_key_value(&relative).or_else(|| {
            self.files
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(&relative))
        });
        let name = match included {
            Some((name, _)) => name.clone(),
            None if access == Access::Import => {
                let file = source_file(self.root, &relative)
                    .map_err(|error| format!("{context}: {error}"))?
                    .ok_or_else(|| {
                        format!("{context}: required file {relative:?} is missing from the project")
                    })?;
                self.files.insert(relative.clone(), file);
                relative
            }
            None => {
                let detail = if self.root.join(&relative).is_file() {
                    "exists in the project but is missing from the package"
                } else {
                    "is missing from the project"
                };
                return Err(format!("{context}: required file {relative:?} {detail}"));
            }
        };
        let next = match access {
            Access::Import => Source::new(name, SourceKind::Library),
            Access::Chain => Source::new(name, SourceKind::Program),
            Access::Merge => Source {
                name,
                base: source.base.clone(),
                kind: SourceKind::Merge,
            },
            _ => return Ok(()),
        };
        self.pending.push(next);
        Ok(())
    }
}

fn file_expression(command: &str) -> Option<(&'static str, Access, String)> {
    let (word, arguments) = command.split_once(char::is_whitespace)?;
    let arguments = arguments.trim();
    let (operation, access, arguments, index) = match word.to_ascii_uppercase().as_str() {
        "OPEN" => {
            let for_pos = file_syntax_separator(arguments, "FOR")?;
            let rest = arguments[for_pos + 3..].trim();
            let as_pos = file_syntax_separator(rest, "AS")?;
            let (operation, access) = match rest[..as_pos].trim().to_ascii_uppercase().as_str() {
                "INPUT" => ("OPEN FOR INPUT", Access::DataRead),
                "OUTPUT" => ("OPEN FOR OUTPUT", Access::DataWrite),
                "APPEND" => ("OPEN FOR APPEND", Access::DataWrite),
                _ => return None,
            };
            return Some((operation, access, arguments[..for_pos].trim().to_string()));
        }
        "AUDIO" => {
            let (verb, rest) = arguments.split_once(char::is_whitespace)?;
            if !verb.eq_ignore_ascii_case("LOAD") {
                return None;
            }
            ("AUDIO LOAD", Access::Read, rest, 1)
        }
        "BLOAD" => {
            let end = file_syntax_separator(arguments, "AS").unwrap_or(arguments.len());
            ("BLOAD", Access::ImageRead, &arguments[..end], 0)
        }
        "BSAVE" => ("BSAVE", Access::ImageWrite, arguments, 0),
        "CHAIN" if starts_keyword(&command.to_ascii_uppercase(), "CHAIN MERGE") => {
            ("CHAIN MERGE", Access::Merge, command[11..].trim(), 0)
        }
        "CHAIN" => ("CHAIN", Access::Chain, arguments, 0),
        "MERGE" => ("MERGE", Access::Merge, arguments, 0),
        _ => return None,
    };
    split_arguments(arguments)
        .into_iter()
        .nth(index)
        .map(|expression| (operation, access, expression))
}

fn relative_path(base: &str, path: &str, access: Access) -> Option<String> {
    let image = matches!(access, Access::ImageRead | Access::ImageWrite);
    let path = if image {
        let path = path.trim();
        path.strip_prefix('"')
            .and_then(|path| path.strip_suffix('"'))
            .unwrap_or(path)
    } else {
        path
    };
    let path = path.replace('\\', "/");
    if path.trim().is_empty() || path.starts_with("//") || has_windows_drive_prefix(&path) {
        return None;
    }
    let mut parts = if path.starts_with('/') || base.is_empty() {
        Vec::new()
    } else {
        base.split('/').collect()
    };
    for part in path.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                if parts.pop().is_none() && matches!(access, Access::DataRead | Access::DataWrite) {
                    return None;
                }
            }
            _ => parts.push(part),
        }
    }
    let name = parts.last()?;
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str());
    let mut relative = parts.join("/");
    if access.is_program() {
        match extension {
            None => relative.push_str(".bas"),
            Some(extension) if extension.eq_ignore_ascii_case("bas") => (),
            _ => return None,
        }
    } else if image && !extension.is_some_and(|extension| extension.eq_ignore_ascii_case("png")) {
        return None;
    }
    valid_relative_path(&relative).then_some(relative)
}

fn source_file(root: &Path, relative: &str) -> Result<Option<PathBuf>, String> {
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        path = Interpreter::match_existing_case(path.join(component));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("cannot check {}: {error}", path.display())),
        };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x0400 != 0 {
                return Err(format!("cannot package link {}", path.display()));
            }
        }
        if metadata.file_type().is_symlink() {
            return Err(format!("cannot package link {}", path.display()));
        }
    }
    if !path.is_file() {
        return Ok(None);
    }
    let path = path
        .canonicalize()
        .map_err(|error| format!("cannot check {}: {error}", path.display()))?;
    if !path.starts_with(root) {
        return Err(format!("cannot package a file outside {}", root.display()));
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, name: &str, content: &str) -> PathBuf {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn checks_exact_inventory_and_reports_calculated_paths() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let main = write(
            &root,
            "main.bas",
            concat!(
                "10 OPEN \"records.csv\" FOR INPUT AS #1\n",
                "20 IF X THEN BLOAD \"image.png\",A$ AS RGBA ELSE AUDIO LOAD 1,\"sound.wav\"\n",
                "30 OPEN \"records.csv\" FOR OUTPUT AS #1\n",
                "40 OPEN LEVEL$ FOR INPUT AS #2\n",
                "50 ' BLOAD \"irrelevant.png\"\n",
            ),
        );
        let image = write(&root, "image.png", "");
        let sound = write(&root, "sound.wav", "");
        let mut files = BTreeMap::from([("main.bas".into(), main), ("image.png".into(), image)]);
        let persistent = ["RECORDS.CSV".to_string()];
        let error = validate_pack(&root, "main.bas", &mut files, &persistent).unwrap_err();
        assert!(error.contains("main.bas:20 AUDIO LOAD"), "{error}");
        assert!(
            error.contains("exists in the project but is missing from the package"),
            "{error}"
        );

        files.insert("sound.wav".into(), sound);
        assert_eq!(
            validate_pack(&root, "main.bas", &mut files, &persistent).unwrap(),
            ["main.bas:40 OPEN FOR INPUT: cannot verify the calculated path LEVEL$"]
        );
    }

    #[test]
    fn auto_includes_imports_and_checks_the_right_program_directories() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let main = write(
            &root,
            "main.bas",
            "10 IMPORT \"lib/helper\" AS Helper\n20 CHAIN \"levels/next\"\n",
        );
        let helper = write(
            &root,
            "lib/helper.bas",
            "10 DEF SUB ReadData()\n20 OPEN \"module.csv\" FOR INPUT AS #1\n30 SUBEND\n",
        );
        let mut files = BTreeMap::from([
            ("main.bas".into(), main),
            ("lib/module.csv".into(), write(&root, "lib/module.csv", "")),
            (
                "levels/next.bas".into(),
                write(
                    &root,
                    "levels/next.bas",
                    "10 CHAIN MERGE \"/fragments/extra.bas\"\n",
                ),
            ),
            (
                "fragments/extra.bas".into(),
                write(
                    &root,
                    "fragments/extra.bas",
                    "20 OPEN \"level.csv\" FOR INPUT AS #1\n",
                ),
            ),
            (
                "levels/level.csv".into(),
                write(&root, "levels/level.csv", ""),
            ),
        ]);
        assert!(validate_pack(&root, "main.bas", &mut files, &[])
            .unwrap()
            .is_empty());
        assert_eq!(files.get("lib/helper.bas"), Some(&helper));
    }
}
