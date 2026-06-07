use std::env;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::Digest;

use crate::archive::{extract_artifact_archive, move_dir, TempDir};
use crate::args::{
    download_spec_for_language, ArtifactLanguage, CacheCliOptions, CliError, DownloadCliOptions,
    WhereCliOptions,
};
use crate::commands::unidic_artifact::verify_unidic_artifact_bundle;
use crate::commands::zh_artifact::verify_zh_artifact_bundle;

const DOWNLOAD_TIMEOUT_SECS: u64 = 60;
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CHECKSUM_MANIFEST_BYTES: u64 = 1024 * 1024;

pub(crate) fn run_download(options: DownloadCliOptions) -> Result<(), Box<dyn Error>> {
    let cache_dir = options
        .cache_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(default_cache_dir);
    let archive_url = options.url.as_deref().unwrap_or(options.spec.archive_url);
    let archive_name = uri_file_name(archive_url).unwrap_or(options.spec.archive_name);
    let temp = TempDir::new("moine-download")?;
    let archive_path = temp.path().join(archive_name);

    copy_uri_to_path(archive_url, &archive_path)?;
    if let Some(expected_sha256) = download_expected_sha256(&options, archive_name)? {
        let actual_sha256 = sha256_file(&archive_path)?;
        if actual_sha256 != expected_sha256 {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "checksum mismatch for {archive_name}: expected {expected_sha256}, got {actual_sha256}"
            ))));
        }
    }

    let extracted_root = extract_artifact_archive(&archive_path, &temp.path().join("extract"))?;
    let metadata = extracted_root.join("metadata.yaml");
    if !metadata.is_file() {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "downloaded artifact has no metadata.yaml: {}",
            extracted_root.display()
        ))));
    }
    verify_downloaded_bundle(options.spec.language, &metadata)?;

    fs::create_dir_all(&cache_dir)?;
    let destination = cache_dir.join(extracted_root.file_name().ok_or_else(|| {
        CliError::ArtifactVerificationFailed(format!(
            "extracted artifact root {} has no file name",
            extracted_root.display()
        ))
    })?);
    if destination.exists() {
        if !options.force {
            println!("{}", destination.display());
            return Ok(());
        }
        fs::remove_dir_all(&destination)?;
    }
    move_dir(&extracted_root, &destination)?;
    println!("{}", destination.display());
    Ok(())
}

pub(crate) fn run_download_list(options: CacheCliOptions) -> Result<(), Box<dyn Error>> {
    let cache_dir = options
        .cache_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_cache_dir);
    for metadata in installed_metadata_paths(&cache_dir)? {
        if let Some(parent) = metadata.parent() {
            println!("{}", parent.display());
        }
    }
    Ok(())
}

pub(crate) fn run_download_where(options: WhereCliOptions) -> Result<(), Box<dyn Error>> {
    let cache_dir = options
        .cache_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_cache_dir);
    let Some(language) = options.language else {
        println!("{}", cache_dir.display());
        return Ok(());
    };
    let spec = download_spec_for_language(language);
    if let Some(metadata) = find_metadata_by_prefix(&cache_dir, spec.artifact_name)? {
        if let Some(parent) = metadata.parent() {
            println!("{}", parent.display());
            return Ok(());
        }
    }
    println!("{}", cache_dir.join(spec.artifact_name).display());
    Ok(())
}

pub(crate) fn default_cache_dir() -> PathBuf {
    if let Some(cache_dir) = env::var_os("MOINE_CACHE_DIR") {
        return PathBuf::from(cache_dir);
    }
    if let Some(cache_home) = env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(cache_home).join("moine").join("dictionaries");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("moine")
            .join("dictionaries");
    }
    PathBuf::from(".moine").join("dictionaries")
}

pub(crate) fn uri_file_name(uri: &str) -> Option<&str> {
    uri.rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && !name.contains('\\'))
}

pub(crate) fn copy_uri_to_path(uri: &str, output: &Path) -> Result<(), Box<dyn Error>> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        let response = ureq::get(uri)
            .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
            .call()?;
        let mut reader = response.into_reader();
        let mut file = fs::File::create(output)?;
        let copied = std::io::copy(&mut reader.by_ref().take(MAX_DOWNLOAD_BYTES + 1), &mut file)?;
        if copied > MAX_DOWNLOAD_BYTES {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "download exceeded maximum size of {MAX_DOWNLOAD_BYTES} bytes"
            ))));
        }
        return Ok(());
    }
    if let Some(path) = uri.strip_prefix("file://") {
        fs::copy(path, output)?;
        return Ok(());
    }
    fs::copy(uri, output)?;
    Ok(())
}

pub(crate) fn read_uri_text(uri: &str) -> Result<String, Box<dyn Error>> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        let response = ureq::get(uri)
            .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
            .call()?;
        let mut text = String::new();
        let read = response
            .into_reader()
            .take(MAX_CHECKSUM_MANIFEST_BYTES + 1)
            .read_to_string(&mut text)?;
        if read as u64 > MAX_CHECKSUM_MANIFEST_BYTES {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "checksum manifest exceeded maximum size of {MAX_CHECKSUM_MANIFEST_BYTES} bytes"
            ))));
        }
        return Ok(text);
    }
    if let Some(path) = uri.strip_prefix("file://") {
        return Ok(fs::read_to_string(path)?);
    }
    Ok(fs::read_to_string(uri)?)
}

pub(crate) fn expected_sha256(
    checksum_url: &str,
    archive_name: &str,
) -> Result<String, Box<dyn Error>> {
    for line in read_uri_text(checksum_url)?.lines() {
        let mut parts = line.split_whitespace();
        let Some(digest) = parts.next() else {
            continue;
        };
        let Some(label) = parts.next() else {
            continue;
        };
        if parts.next().is_some() {
            continue;
        }
        if label == archive_name
            || Path::new(label).file_name().and_then(|name| name.to_str()) == Some(archive_name)
        {
            return Ok(digest.to_ascii_lowercase());
        }
    }
    Err(Box::new(CliError::ArtifactVerificationFailed(format!(
        "{archive_name} not found in checksum manifest: {checksum_url}"
    ))))
}

pub(crate) fn download_expected_sha256(
    options: &DownloadCliOptions,
    archive_name: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    if let Some(value) = &options.sha256 {
        return Ok(Some(value.to_ascii_lowercase()));
    }
    if let Some(checksum_url) = options
        .checksum_url
        .as_deref()
        .or(options.spec.checksum_url)
    {
        return Ok(Some(expected_sha256(checksum_url, archive_name)?));
    }
    Ok(None)
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = fs::File::open(path)?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sha2::Digest::update(&mut digest, &buffer[..read]);
    }
    Ok(format!("{:x}", sha2::Digest::finalize(digest)))
}

pub(crate) fn verify_downloaded_bundle(
    language: ArtifactLanguage,
    metadata: &Path,
) -> Result<(), Box<dyn Error>> {
    let metadata = metadata.to_str().ok_or_else(|| {
        CliError::ArtifactVerificationFailed("metadata path is not UTF-8".to_string())
    })?;
    match language {
        ArtifactLanguage::Japanese => {
            verify_unidic_artifact_bundle(metadata, None, false)?;
        }
        ArtifactLanguage::Chinese => {
            verify_zh_artifact_bundle(metadata, None)?;
        }
    }
    Ok(())
}

pub(crate) fn installed_metadata_paths(cache_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut metadata_paths = Vec::new();
    if !cache_dir.is_dir() {
        return Ok(metadata_paths);
    }
    let root_metadata = cache_dir.join("metadata.yaml");
    if root_metadata.is_file() {
        metadata_paths.push(root_metadata);
    }
    for entry in fs::read_dir(cache_dir)? {
        let path = entry?.path();
        let metadata = path.join("metadata.yaml");
        if path.is_dir() && metadata.is_file() {
            metadata_paths.push(metadata);
        }
    }
    metadata_paths.sort();
    metadata_paths.dedup();
    Ok(metadata_paths)
}

pub(crate) fn find_metadata_by_prefix(
    cache_dir: &Path,
    artifact_name: &str,
) -> Result<Option<PathBuf>, Box<dyn Error>> {
    Ok(installed_metadata_paths(cache_dir)?
        .into_iter()
        .find(|metadata| {
            metadata
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(artifact_name))
        }))
}
