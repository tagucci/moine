use std::env;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process;
use std::process::{Command, Stdio};
use std::time::Duration;

use flate2::{read::GzDecoder, write::GzEncoder, Compression, GzBuilder};

use crate::args::{ArchiveCompression, CliError};

const ZSTD_COMPRESSION_LEVEL: i32 = 19;

pub(crate) struct ArchiveEntry {
    pub(crate) source: PathBuf,
    pub(crate) path: String,
}

pub(crate) fn ensure_output_parent(path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub(crate) fn write_output_file(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> Result<(), Box<dyn Error>> {
    let path = path.as_ref();
    ensure_output_parent(path)?;
    fs::write(path, contents)?;
    Ok(())
}

pub(crate) fn create_output_file(path: impl AsRef<Path>) -> Result<fs::File, Box<dyn Error>> {
    let path = path.as_ref();
    ensure_output_parent(path)?;
    Ok(fs::File::create(path)?)
}

pub(crate) fn extract_artifact_archive(
    archive: &Path,
    output_dir: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CliError::ArtifactVerificationFailed(format!(
                "artifact archive path is not UTF-8: {}",
                archive.display()
            ))
        })?;
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let file = fs::File::open(archive)?;
        let decoder = GzDecoder::new(file);
        return extract_tar_stream(decoder, output_dir);
    }
    if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        let file = fs::File::open(archive)?;
        let decoder = zstd::stream::read::Decoder::new(file)?;
        return extract_tar_stream(decoder, output_dir);
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        return extract_xz_tar_archive(archive, output_dir);
    }
    if name.ends_with(".tar") {
        let file = fs::File::open(archive)?;
        return extract_tar_stream(file, output_dir);
    }
    Err(Box::new(CliError::ArtifactVerificationFailed(format!(
        "unsupported artifact archive extension: {name}"
    ))))
}

pub(crate) fn extract_xz_tar_archive(
    archive: &Path,
    output_dir: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    let mut child = Command::new("xz")
        .arg("-dc")
        .arg(archive)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|err| {
            CliError::ArtifactVerificationFailed(format!(
                "failed to start xz for {}: {err}",
                archive.display()
            ))
        })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        CliError::ArtifactVerificationFailed("failed to capture xz stdout".to_string())
    })?;
    let extracted = extract_tar_stream(stdout, output_dir);
    let status = child.wait()?;
    let extracted = extracted?;
    if !status.success() {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "xz failed for {} with status {status}",
            archive.display()
        ))));
    }
    Ok(extracted)
}

pub(crate) fn extract_tar_stream(
    reader: impl Read,
    output_dir: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    fs::create_dir_all(output_dir)?;
    let mut archive = tar::Archive::new(reader);
    let mut root_name = None::<PathBuf>;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "unsupported archive entry type for {}",
                entry.path()?.display()
            ))));
        }
        let path = entry.path()?.to_path_buf();
        let first_component = path.components().next().ok_or_else(|| {
            CliError::ArtifactVerificationFailed("empty archive path".to_string())
        })?;
        let std::path::Component::Normal(root_component) = first_component else {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "unsafe archive path: {}",
                path.display()
            ))));
        };
        let current_root = PathBuf::from(root_component);
        match &root_name {
            Some(root) if root != &current_root => {
                return Err(Box::new(CliError::ArtifactVerificationFailed(
                    "artifact archive must contain exactly one top-level directory".to_string(),
                )));
            }
            None => root_name = Some(current_root),
            _ => {}
        }
        if path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        }) || path.is_absolute()
        {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "unsafe archive path: {}",
                path.display()
            ))));
        }
        let target = output_dir.join(&path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(target)?;
    }
    let root = root_name.ok_or_else(|| {
        CliError::ArtifactVerificationFailed("artifact archive is empty".to_string())
    })?;
    Ok(output_dir.join(root))
}

pub(crate) fn move_dir(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_dir_all(source, destination)?;
            fs::remove_dir_all(source)?;
            Ok(())
        }
    }
}

pub(crate) fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_all(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

pub(crate) struct TempDir {
    pub(crate) path: PathBuf,
}

impl TempDir {
    pub(crate) fn new(prefix: &str) -> Result<Self, std::io::Error> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{}-{nanos}", process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn checked_bundle_path(
    bundle_dir: &Path,
    relative_path: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    let relative = Path::new(relative_path);
    if relative_path.contains('\\')
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "bundle path {relative_path:?} must be relative and stay inside the bundle"
        ))));
    }
    Ok(bundle_dir.join(relative))
}

pub(crate) fn normalized_relative_archive_path(
    relative_path: &str,
) -> Result<String, Box<dyn Error>> {
    checked_bundle_path(Path::new(""), relative_path)?;
    Ok(relative_path.to_string())
}

pub(crate) fn release_checksum_asset_label(path: &Path) -> Result<&str, Box<dyn Error>> {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .filter(|file_name| !file_name.is_empty())
        .ok_or_else(|| {
            Box::new(CliError::ArtifactVerificationFailed(format!(
                "release asset path {} has no file name",
                path.display()
            ))) as Box<dyn Error>
        })
}

pub(crate) fn write_release_archive(
    writer: impl std::io::Write,
    compression: ArchiveCompression,
    root_name: &str,
    entries: &[ArchiveEntry],
) -> Result<(), Box<dyn Error>> {
    match compression {
        ArchiveCompression::None => {
            let mut writer = writer;
            write_tar_archive(&mut writer, root_name, entries)?;
        }
        ArchiveCompression::Gzip => {
            let mut encoder = gzip_encoder(writer);
            write_tar_archive(&mut encoder, root_name, entries)?;
            encoder.finish()?;
        }
        ArchiveCompression::Zstd => {
            let mut encoder = zstd::stream::write::Encoder::new(writer, ZSTD_COMPRESSION_LEVEL)?;
            write_tar_archive(&mut encoder, root_name, entries)?;
            encoder.finish()?;
        }
    }
    Ok(())
}

pub(crate) fn gzip_encoder(writer: impl std::io::Write) -> GzEncoder<impl std::io::Write> {
    GzBuilder::new()
        .mtime(0)
        .write(writer, Compression::default())
}

pub(crate) fn write_tar_archive(
    writer: &mut impl std::io::Write,
    root_name: &str,
    entries: &[ArchiveEntry],
) -> Result<(), Box<dyn Error>> {
    let root_name = sanitize_archive_root(root_name)?;
    for entry in entries {
        let data = fs::read(&entry.source)?;
        let archive_path = format!("{root_name}/{}", entry.path);
        write_tar_file_entry(writer, &archive_path, &data)?;
    }
    writer.write_all(&[0_u8; 1024])?;
    Ok(())
}

pub(crate) fn sanitize_archive_root(root_name: &str) -> Result<String, Box<dyn Error>> {
    if root_name.is_empty()
        || root_name == "."
        || root_name == ".."
        || root_name.contains('/')
        || root_name.contains('\\')
    {
        return Err(Box::new(CliError::InvalidArgumentValue {
            name: "--root-name",
            value: root_name.to_string(),
            expected: "single relative path component",
        }));
    }
    Ok(root_name.to_string())
}

pub(crate) fn write_tar_file_entry(
    writer: &mut impl std::io::Write,
    path: &str,
    data: &[u8],
) -> Result<(), Box<dyn Error>> {
    let mut header = [0_u8; 512];
    write_tar_name(&mut header, path)?;
    write_tar_octal(&mut header[100..108], 0o644)?;
    write_tar_octal(&mut header[108..116], 0)?;
    write_tar_octal(&mut header[116..124], 0)?;
    write_tar_octal(&mut header[124..136], data.len() as u64)?;
    write_tar_octal(&mut header[136..148], 0)?;
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum = header.iter().map(|byte| u32::from(*byte)).sum::<u32>() as u64;
    write_tar_checksum(&mut header[148..156], checksum)?;

    writer.write_all(&header)?;
    writer.write_all(data)?;
    let padding = (512 - (data.len() % 512)) % 512;
    if padding > 0 {
        writer.write_all(&vec![0_u8; padding])?;
    }
    Ok(())
}

pub(crate) fn write_tar_name(header: &mut [u8; 512], path: &str) -> Result<(), Box<dyn Error>> {
    let bytes = path.as_bytes();
    if bytes.len() > 100 {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "archive path {path:?} exceeds the current 100-byte tar name limit"
        ))));
    }
    header[..bytes.len()].copy_from_slice(bytes);
    Ok(())
}

pub(crate) fn write_tar_octal(field: &mut [u8], value: u64) -> Result<(), Box<dyn Error>> {
    let digits = field.len() - 1;
    let text = format!("{value:0digits$o}");
    if text.len() > digits {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "tar numeric field overflow for value {value}"
        ))));
    }
    field[..digits].copy_from_slice(text.as_bytes());
    field[digits] = 0;
    Ok(())
}

pub(crate) fn write_tar_checksum(field: &mut [u8], value: u64) -> Result<(), Box<dyn Error>> {
    let text = format!("{value:06o}\0 ");
    if text.len() != field.len() {
        return Err(Box::new(CliError::ArtifactVerificationFailed(
            "tar checksum field overflow".to_string(),
        )));
    }
    field.copy_from_slice(text.as_bytes());
    Ok(())
}

pub(crate) fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
