use super::{valid_relative_path, validate_manifest, Manifest};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::{NamedTempFile, TempDir};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const PACK_COMMENT: &str = "AVL BASIC PACK ZIP v1";
const MANIFEST_NAME: &str = "manifest.json";
const PROGRAM_PREFIX: &str = "program/";
const MAX_MANIFEST_SIZE: u64 = 1024 * 1024;

pub(super) fn write(
    executable: &Path,
    output: &Path,
    files: &BTreeMap<String, PathBuf>,
    manifest: &Manifest,
) -> Result<(), String> {
    validate_manifest(manifest)?;
    if !files.contains_key(&manifest.entry) {
        return Err(format!("Missing PACK entry {:?}.", manifest.entry));
    }
    let manifest_bytes = serde_json::to_vec(manifest).map_err(|error| error.to_string())?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_SIZE {
        return Err("PACK manifest is too large.".into());
    }

    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut staged = NamedTempFile::new_in(parent)
        .map_err(|error| format!("Could not prepare {}: {error}", output.display()))?;
    let mut template = File::open(executable)
        .map_err(|error| format!("Could not open {}: {error}", executable.display()))?;
    if has_pack_comment(&mut template).map_err(|error| error.to_string())? {
        return Err("The PACK template is already a packaged executable.".into());
    }
    template.rewind().map_err(|error| error.to_string())?;
    let executable_size = io::copy(&mut template, staged.as_file_mut())
        .map_err(|error| format!("Could not copy {}: {error}", executable.display()))?;
    if manifest.gui {
        set_gui_subsystem(staged.as_file_mut(), executable_size)
            .map_err(|error| format!("Could not prepare the GUI executable: {error}"))?;
    }

    let mut archive = ZipWriter::new(staged.as_file_mut());
    archive
        .set_comment(PACK_COMMENT)
        .map_err(|error| error.to_string())?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in [
        (MANIFEST_NAME, manifest_bytes.as_slice()),
        ("COPYING", include_bytes!("../../COPYING").as_slice()),
        ("LICENSES", include_bytes!("../../LICENSES").as_slice()),
    ] {
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        archive
            .write_all(bytes)
            .map_err(|error| error.to_string())?;
    }
    for (name, path) in files {
        if !valid_relative_path(name) {
            return Err(format!("Invalid PACK path {name:?}."));
        }
        let mut source = File::open(path)
            .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
        let size = source.metadata().map_err(|error| error.to_string())?.len();
        archive
            .start_file(
                format!("{PROGRAM_PREFIX}{name}"),
                options.large_file(size >= u32::MAX.into()),
            )
            .map_err(|error| format!("Could not pack {name:?}: {error}"))?;
        io::copy(&mut source, &mut archive)
            .map_err(|error| format!("Could not pack {name:?}: {error}"))?;
    }
    archive.finish().map_err(|error| error.to_string())?;
    staged
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    staged
        .persist(output)
        .map_err(|error| format!("Could not write {}: {error}", output.display()))?;
    Ok(())
}

fn set_gui_subsystem(executable: &mut (impl Read + Write + Seek), size: u64) -> io::Result<()> {
    const DOS_HEADER_LEN: usize = 64;
    const PE_HEADER_LEN: usize = 24; // Signature followed by the COFF header.
    const PE32_PLUS_FIXED_LEN: u64 = 112;
    const PE32_PLUS_MAGIC: u16 = 0x20b;
    const CHECKSUM_OFFSET: usize = 64;
    const SUBSYSTEM_OFFSET: usize = 68;
    const AMD64: u16 = 0x8664;
    const EXECUTABLE_IMAGE: u16 = 0x0002;
    const DLL: u16 = 0x2000;
    const WINDOWS_GUI: u16 = 2;
    const WINDOWS_CUI: u16 = 3;

    let invalid_template = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "expected a Windows x64 console executable",
        )
    };
    let mut dos = [0; DOS_HEADER_LEN];
    executable.rewind()?;
    executable.read_exact(&mut dos)?;
    let pe_offset = u64::from(u32::from_le_bytes(dos[0x3c..0x40].try_into().unwrap()));
    let optional_offset = pe_offset + PE_HEADER_LEN as u64;
    if &dos[..2] != b"MZ" || pe_offset < DOS_HEADER_LEN as u64 || optional_offset > size {
        return Err(invalid_template());
    }

    let mut pe = [0; PE_HEADER_LEN];
    executable.seek(SeekFrom::Start(pe_offset))?;
    executable.read_exact(&mut pe)?;
    let machine = u16::from_le_bytes([pe[4], pe[5]]);
    let optional_len = u64::from(u16::from_le_bytes([pe[20], pe[21]]));
    let characteristics = u16::from_le_bytes([pe[22], pe[23]]);
    if &pe[..4] != b"PE\0\0"
        || machine != AMD64
        || characteristics & (EXECUTABLE_IMAGE | DLL) != EXECUTABLE_IMAGE
        || optional_len < PE32_PLUS_FIXED_LEN
        || optional_offset + optional_len > size
    {
        return Err(invalid_template());
    }

    let mut optional = [0; SUBSYSTEM_OFFSET + 2];
    executable.read_exact(&mut optional)?;
    let magic = u16::from_le_bytes([optional[0], optional[1]]);
    let subsystem = u16::from_le_bytes([optional[SUBSYSTEM_OFFSET], optional[SUBSYSTEM_OFFSET + 1]]);
    if magic != PE32_PLUS_MAGIC || subsystem != WINDOWS_CUI {
        return Err(invalid_template());
    }

    // Preserve the console template's entry point. Clear the old checksum
    // because the header and the appended ZIP change the image on disk.
    executable.seek(SeekFrom::Start(optional_offset + CHECKSUM_OFFSET as u64))?;
    executable.write_all(&0u32.to_le_bytes())?;
    executable.write_all(&WINDOWS_GUI.to_le_bytes())?;
    executable.seek(SeekFrom::Start(size))?;
    Ok(())
}

pub(super) fn extract(executable: &Path) -> Result<Option<(TempDir, Manifest)>, String> {
    let file = File::open(executable)
        .map_err(|error| format!("Could not open {}: {error}", executable.display()))?;
    let mut reader = BufReader::new(file);
    if !has_pack_comment(&mut reader).map_err(|error| error.to_string())? {
        return Ok(None);
    }

    let mut archive = ZipArchive::new(reader).map_err(|error| format!("Invalid PACK: {error}"))?;
    if archive.comment() != PACK_COMMENT.as_bytes() {
        return Err("Invalid PACK marker.".into());
    }
    let manifest = read_manifest(&mut archive)?;
    let directory = tempfile::Builder::new()
        .prefix("avl-basic-pack-")
        .tempdir()
        .map_err(|error| format!("Could not create the PACK directory: {error}"))?;

    for index in 0..archive.len() {
        let member = archive.by_index(index).map_err(|error| error.to_string())?;
        let name =
            std::str::from_utf8(member.name_raw()).map_err(|_| "Invalid UTF-8 in PACK path.")?;
        if matches!(name, MANIFEST_NAME | "COPYING" | "LICENSES") {
            continue;
        }
        let relative = name
            .strip_prefix(PROGRAM_PREFIX)
            .filter(|path| valid_relative_path(path) && member.is_file())
            .ok_or_else(|| format!("Invalid PACK entry {name:?}."))?
            .to_owned();
        let path = directory.path().join(&relative);
        let parent = path
            .parent()
            .ok_or_else(|| format!("Invalid PACK path {relative:?}."))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not extract {relative:?}: {error}"))?;
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("Could not extract {relative:?}: {error}"))?;
        let size = member.size();
        let copied = io::copy(&mut member.take(size.saturating_add(1)), &mut destination)
            .map_err(|error| format!("Could not extract {relative:?}: {error}"))?;
        if copied != size {
            return Err(format!("Invalid PACK size for {relative:?}."));
        }
    }

    if !directory.path().join(&manifest.entry).is_file() {
        return Err(format!("Missing PACK entry {:?}.", manifest.entry));
    }
    Ok(Some((directory, manifest)))
}

fn has_pack_comment(reader: &mut (impl Read + Seek)) -> io::Result<bool> {
    let end = reader.seek(SeekFrom::End(0))?;
    let mut marker = [0; PACK_COMMENT.len()];
    if end < marker.len() as u64 {
        return Ok(false);
    }
    reader.seek(SeekFrom::Start(end - marker.len() as u64))?;
    reader.read_exact(&mut marker)?;
    Ok(marker == PACK_COMMENT.as_bytes())
}

fn read_manifest(archive: &mut ZipArchive<impl Read + Seek>) -> Result<Manifest, String> {
    let member = archive
        .by_name(MANIFEST_NAME)
        .map_err(|error| format!("Could not read the PACK manifest: {error}"))?;
    if !member.is_file() || member.size() > MAX_MANIFEST_SIZE {
        return Err("Invalid PACK manifest size or type.".into());
    }
    let mut bytes = Vec::new();
    member
        .take(MAX_MANIFEST_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read the PACK manifest: {error}"))?;
    if bytes.len() as u64 > MAX_MANIFEST_SIZE {
        return Err("PACK manifest is too large.".into());
    }
    let manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid PACK manifest: {error}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_zip_keeps_the_template_and_recovers_the_project_tree() {
        let temp = tempfile::tempdir().unwrap();
        let template = temp.path().join("runtime.exe");
        let program = temp.path().join("main.bas");
        let asset = temp.path().join("level.txt");
        let output = temp.path().join("game.exe");
        let prefix = b"MZ\x00runtime bytes";
        let source = b"10 END\n";
        fs::write(&template, prefix).unwrap();
        fs::write(&program, source).unwrap();
        fs::write(&asset, b"level data").unwrap();
        assert!(extract(&template).unwrap().is_none());

        let manifest = Manifest {
            entry: "main.bas".into(),
            id: "example.game".into(),
            persistent: vec!["score.csv".into()],
            gui: false,
        };
        let files = BTreeMap::from([
            (manifest.entry.clone(), program),
            ("assets/level.txt".into(), asset),
        ]);
        write(&template, &output, &files, &manifest).unwrap();

        assert!(fs::read(&output).unwrap().starts_with(prefix));
        let (extracted, unpacked_manifest) = extract(&output).unwrap().unwrap();
        assert_eq!(unpacked_manifest, manifest);
        assert_eq!(fs::read(extracted.path().join("main.bas")).unwrap(), source);
        assert_eq!(
            fs::read(extracted.path().join("assets/level.txt")).unwrap(),
            b"level data"
        );
    }

    #[test]
    fn failed_pack_preserves_the_previous_executable() {
        let temp = tempfile::tempdir().unwrap();
        let template = temp.path().join("runtime.exe");
        let output = temp.path().join("game.exe");
        fs::write(&template, b"MZ\x00runtime bytes").unwrap();
        fs::write(&output, b"previous output").unwrap();
        let manifest = Manifest {
            entry: "main.bas".into(),
            id: "example.game".into(),
            persistent: Vec::new(),
            gui: false,
        };
        let files = BTreeMap::from([(manifest.entry.clone(), temp.path().join("missing.bas"))]);
        assert!(write(&template, &output, &files, &manifest).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"previous output");
    }

    #[test]
    fn gui_pack_changes_the_pe_subsystem_and_keeps_the_zip_readable() {
        let temp = tempfile::tempdir().unwrap();
        let template = temp.path().join("runtime.exe");
        let program = temp.path().join("main.bas");
        let output = temp.path().join("game.exe");
        let pe_offset = 64usize;
        let optional_offset = pe_offset + 24;
        let checksum_offset = optional_offset + 64;
        let subsystem_offset = optional_offset + 68;
        let mut prefix = vec![0; optional_offset + 112];
        prefix[..2].copy_from_slice(b"MZ");
        prefix[0x3c..0x40].copy_from_slice(&(pe_offset as u32).to_le_bytes());
        prefix[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        prefix[pe_offset + 4..pe_offset + 6].copy_from_slice(&0x8664u16.to_le_bytes());
        prefix[pe_offset + 20..pe_offset + 22].copy_from_slice(&112u16.to_le_bytes());
        prefix[pe_offset + 22..optional_offset].copy_from_slice(&2u16.to_le_bytes());
        prefix[optional_offset..optional_offset + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        prefix[optional_offset + 16..optional_offset + 20].copy_from_slice(&0x1234u32.to_le_bytes());
        prefix[checksum_offset..checksum_offset + 4].copy_from_slice(&0x5678u32.to_le_bytes());
        prefix[subsystem_offset..subsystem_offset + 2].copy_from_slice(&3u16.to_le_bytes());
        fs::write(&template, &prefix).unwrap();
        fs::write(&program, b"10 END\n").unwrap();
        let manifest = Manifest {
            entry: "main.bas".into(),
            id: "example.game".into(),
            persistent: Vec::new(),
            gui: true,
        };
        let files = BTreeMap::from([(manifest.entry.clone(), program)]);
        write(&template, &output, &files, &manifest).unwrap();

        assert_eq!(fs::read(&template).unwrap(), prefix);
        prefix[checksum_offset..checksum_offset + 4].fill(0);
        prefix[subsystem_offset..subsystem_offset + 2].copy_from_slice(&2u16.to_le_bytes());
        let game = fs::read(&output).unwrap();
        assert!(game.starts_with(&prefix));
        let (extracted, unpacked_manifest) = extract(&output).unwrap().unwrap();
        assert_eq!(unpacked_manifest, manifest);
        assert_eq!(
            fs::read(extracted.path().join("main.bas")).unwrap(),
            b"10 END\n"
        );

        // A GUI template is rejected without replacing the previous game.
        fs::write(&template, &prefix).unwrap();
        assert!(write(&template, &output, &files, &manifest).is_err());
        assert_eq!(fs::read(&output).unwrap(), game);
    }
}
