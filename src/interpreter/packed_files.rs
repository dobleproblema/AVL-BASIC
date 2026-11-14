use super::{fs, io, BasicError, BasicResult, ErrorCode, FileMode, Interpreter, Path, PathBuf};
use crate::pack::{is_persistent, valid_relative_path};

#[derive(Debug)]
pub(super) struct PackedFiles {
    root: PathBuf,
    save_root: PathBuf,
    persistent: Vec<String>,
}

impl Interpreter {
    /// Sets the virtual root to the extracted application and enables its per-user saves.
    pub fn configure_packed_files(
        &mut self,
        root: PathBuf,
        save_root: PathBuf,
        persistent: Vec<String>,
    ) -> BasicResult<()> {
        let canonical_root = root.canonicalize().map_err(file_error)?;
        if !canonical_root.is_dir() || !save_root.is_absolute() || save_root.parent().is_none() {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        self.root_dir = root;
        self.current_dir = canonical_root.clone();
        self.packed_files = Some(PackedFiles {
            root: canonical_root,
            save_root,
            persistent,
        });
        Ok(())
    }

    pub(super) fn resolve_packed_file(
        &self,
        path: PathBuf,
        mode: FileMode,
    ) -> BasicResult<PathBuf> {
        match &self.packed_files {
            Some(files) => files.resolve(path, mode),
            None => Ok(path),
        }
    }
}

impl PackedFiles {
    fn relative(&self, path: &Path) -> BasicResult<String> {
        let relative = path
            .strip_prefix(&self.root)
            .ok()
            .and_then(Path::to_str)
            .ok_or_else(invalid_path)?
            .replace('\\', "/");
        if !valid_relative_path(&relative) {
            return Err(invalid_path());
        }
        Ok(relative)
    }

    pub(super) fn identity(&self, path: &Path) -> BasicResult<PathBuf> {
        // A bundled seed and its persistent copy are one file to BASIC's channels.
        Ok(PathBuf::from(self.relative(path)?.to_ascii_lowercase()))
    }

    pub(super) fn resolve(&self, path: PathBuf, mode: FileMode) -> BasicResult<PathBuf> {
        let relative = self.relative(&path)?;
        let writing = mode != FileMode::Input;
        if !is_persistent(&relative, &self.persistent) {
            return if writing {
                Err(BasicError::new(ErrorCode::FileIoError).with_detail(format!(
                    "{relative:?} is not declared persistent in this package"
                )))
            } else {
                Ok(path)
            };
        }
        let (user_path, exists) = self.user_file(Path::new(&relative), writing)?;
        if !writing && !exists {
            return Ok(path);
        }
        if mode == FileMode::Append && !exists && path.try_exists().map_err(file_error)? {
            copy_seed(&path, &user_path)?;
        }
        Ok(user_path)
    }

    fn user_file(&self, relative: &Path, writing: bool) -> BasicResult<(PathBuf, bool)> {
        let user_path = self.save_root.join(relative);
        let mut directory = self.save_root.clone();
        if writing {
            fs::create_dir_all(directory.parent().expect("validated save root"))
                .map_err(file_error)?;
        }
        if !ensure_directory(&directory, writing)? {
            return Ok((user_path, false));
        }
        for component in relative
            .parent()
            .expect("validated relative path")
            .components()
        {
            directory.push(component.as_os_str());
            if !ensure_directory(&directory, writing)? {
                return Ok((user_path, false));
            }
        }
        match checked_metadata(&user_path)? {
            Some(metadata) if metadata.is_file() => Ok((user_path, true)),
            None => Ok((user_path, false)),
            Some(_) => Err(invalid_path()),
        }
    }
}

fn invalid_path() -> BasicError {
    BasicError::new(ErrorCode::InvalidArgument)
}

fn file_error(error: io::Error) -> BasicError {
    BasicError::new(ErrorCode::FileIoError).with_detail(error.to_string())
}

fn checked_metadata(path: &Path) -> BasicResult<Option<fs::Metadata>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(file_error(error)),
    };
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    };
    #[cfg(not(windows))]
    let linked = metadata.file_type().is_symlink();
    if linked {
        return Err(invalid_path());
    }
    Ok(Some(metadata))
}

fn ensure_directory(path: &Path, create: bool) -> BasicResult<bool> {
    if let Some(metadata) = checked_metadata(path)? {
        return if metadata.is_dir() {
            Ok(true)
        } else {
            Err(invalid_path())
        };
    }
    if !create {
        return Ok(false);
    }
    match fs::create_dir(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if checked_metadata(path)?.is_some_and(|metadata| metadata.is_dir()) {
                Ok(true)
            } else {
                Err(invalid_path())
            }
        }
        Err(error) => Err(file_error(error)),
    }
}

fn copy_seed(seed: &Path, destination: &Path) -> BasicResult<()> {
    let parent = destination.parent().expect("validated destination");
    let mut copy = tempfile::NamedTempFile::new_in(parent).map_err(file_error)?;
    io::copy(&mut fs::File::open(seed).map_err(file_error)?, &mut copy).map_err(file_error)?;
    // Another launch may have already installed the seed. Keep that copy in this case.
    match copy.persist_noclobber(destination) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            if checked_metadata(destination)?.is_some_and(|metadata| metadata.is_file()) {
                Ok(())
            } else {
                Err(invalid_path())
            }
        }
        Err(error) => Err(file_error(error.error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_survive_another_extraction_and_do_not_need_a_bundled_parent() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile").join("game");
        let write = temporary.path().join("write");
        let read = temporary.path().join("read");
        fs::create_dir(&write).unwrap();
        fs::create_dir(&read).unwrap();
        let persistent = vec!["data/records.csv".to_string()];
        let mut first = Interpreter::new();
        first
            .configure_packed_files(write.clone(), profile.clone(), persistent.clone())
            .unwrap();
        let writer = write.join("main.bas");
        fs::write(
            &writer,
            "10 OPEN \"data/records.csv\" FOR OUTPUT AS #1\n20 WRITE #1,123\n30 CLOSE #1\n40 END\n",
        )
        .unwrap();
        first.load_file(&writer).unwrap();
        first.run_loaded().unwrap();
        assert!(!write.join("data").exists());
        assert_eq!(
            fs::read_to_string(profile.join("data/records.csv")).unwrap(),
            "123\n"
        );

        let mut second = Interpreter::new();
        second
            .configure_packed_files(read, profile, persistent)
            .unwrap();
        for command in [
            "OPEN \"data/records.csv\" FOR INPUT AS #1",
            "INPUT #1,R",
            "CLOSE #1",
            "PRINT R",
        ] {
            second.process_immediate(command).unwrap();
        }
        assert_eq!(second.take_output(), " 123\n");
    }

    #[test]
    fn seeds_keep_their_contents_and_file_channels_share_one_identity() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("bundle");
        let profile = temporary.path().join("profile");
        fs::create_dir(&root).unwrap();
        let seed = root.join("scores.csv");
        fs::write(&seed, "1\n").unwrap();
        fs::write(root.join("immutable.csv"), "0\n").unwrap();
        let mut interpreter = Interpreter::new();
        interpreter
            .configure_packed_files(root, profile.clone(), vec!["scores.csv".to_string()])
            .unwrap();
        interpreter
            .process_immediate("OPEN \"scores.csv\" FOR INPUT AS #1")
            .unwrap();
        let conflict = interpreter
            .process_immediate("OPEN \"/SCORES.CSV\" FOR APPEND AS #2")
            .unwrap_err();
        assert_eq!(conflict.code, ErrorCode::FileAlreadyOpen);
        assert!(!profile.exists());

        for command in [
            "CLOSE #1",
            "OPEN \"scores.csv\" FOR APPEND AS #1",
            "WRITE #1,2",
            "CLOSE #1",
            "OPEN \"scores.csv\" FOR APPEND AS #1",
            "WRITE #1,3",
            "CLOSE #1",
        ] {
            interpreter.process_immediate(command).unwrap();
        }
        assert_eq!(fs::read_to_string(seed).unwrap(), "1\n");
        assert_eq!(
            fs::read_to_string(profile.join("scores.csv")).unwrap(),
            "1\n2\n3\n"
        );
        let denied = interpreter
            .process_immediate("OPEN \"immutable.csv\" FOR OUTPUT AS #1")
            .unwrap_err();
        assert_eq!(denied.code, ErrorCode::FileIoError);
    }
}
