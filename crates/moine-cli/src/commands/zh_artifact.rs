use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use moine_zh::{
    artifact_file_digest_path as zh_artifact_file_digest_path, CedictIndexOptions,
    PinyinReadingOptions, ZhArtifactLicense, ZhArtifactMetadata, ZhArtifactMetadataOptions,
    ZhReadingIndex, ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM as ZH_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM,
    ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM as ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM,
};

use crate::archive::{
    checked_bundle_path, create_output_file, normalized_relative_archive_path,
    release_checksum_asset_label, write_output_file, write_release_archive, ArchiveEntry,
};
use crate::args::{
    default_zh_payload_file_name, ArtifactPayloadFormat, CliError, ZhArtifactArchiveCliOptions,
    ZhArtifactBundleCliOptions, ZhArtifactInspectCliOptions, ZhArtifactMetadataCliOptions,
    ZhArtifactPayloadCliOptions, ZhArtifactReleaseChecksumsCliOptions, ZhArtifactVerifyCliOptions,
    ZhIndexSource, INDEXED_PAYLOAD_FORMAT, YAML_PAYLOAD_FORMAT,
};

pub(crate) fn run_zh_artifact_metadata(
    options: ZhArtifactMetadataCliOptions,
) -> Result<(), Box<dyn Error>> {
    let reading_options = validate_zh_dictionary_options(options.reading_options)?;
    let index =
        ZhReadingIndex::from_cedict_path_with_options(&options.cedict, options.index_options)?;
    let metadata = index.artifact_metadata(ZhArtifactMetadataOptions {
        artifact_name: options.artifact_name,
        generator: "moine-cli".to_string(),
        payload_file_name: options.payload_file_name,
        payload_format: options.payload_format.as_str().to_string(),
        source_name: options.source_name,
        source_version: options.source_version,
        source_cedict: options.cedict.clone(),
        index_options: options.index_options,
        query_defaults: reading_options,
        license: ZhArtifactLicense::default(),
    });
    let yaml = serde_yaml::to_string(&metadata)?;

    if let Some(output) = &options.output {
        write_output_file(output, yaml)?;
    } else {
        print!("{yaml}");
    }

    Ok(())
}

pub(crate) fn run_zh_artifact_payload(
    options: ZhArtifactPayloadCliOptions,
) -> Result<(), Box<dyn Error>> {
    let index =
        ZhReadingIndex::from_cedict_path_with_options(&options.cedict, options.index_options)?;

    if let Some(output) = &options.output {
        write_zh_artifact_payload_file(&index, options.payload_format, Path::new(output))?;
    } else {
        if options.payload_format != ArtifactPayloadFormat::Yaml {
            return Err(Box::new(CliError::ArtifactVerificationFailed(
                "non-YAML zh payload output requires --output".to_string(),
            )));
        }
        print!("{}", serde_yaml::to_string(&index.artifact_payload())?);
    }

    Ok(())
}

pub(crate) fn run_zh_artifact_archive(
    options: ZhArtifactArchiveCliOptions,
) -> Result<(), Box<dyn Error>> {
    let verified = verify_zh_artifact_bundle(&options.metadata, options.bundle_dir.as_deref())?;
    let root_name = options
        .root_name
        .unwrap_or_else(|| verified.metadata.artifact_name.clone());
    let entries = zh_release_archive_entries(&verified)?;
    let output = create_output_file(&options.output)?;
    write_release_archive(output, options.compression, &root_name, &entries)?;

    println!("archive: {}", options.output);
    println!("compression: {}", options.compression.as_str());
    println!("root: {}", root_name);
    println!("metadata: {}", verified.metadata_path.display());
    println!("bundle: {}", verified.bundle_dir.display());
    println!("entries: {}", entries.len());
    println!("verified: true");

    Ok(())
}

pub(crate) fn run_zh_artifact_inspect(
    options: ZhArtifactInspectCliOptions,
) -> Result<(), Box<dyn Error>> {
    let index = load_zh_artifact_payload_by_format(
        Path::new(&options.payload),
        options.payload_format.as_str(),
    )?;

    println!("payload: {}", options.payload);
    println!("format: {}", options.payload_format.as_str());
    println!("pinyin_view: {}", index.pinyin_view().as_str());
    println!("entries: {}", index.len());
    println!("checksum_algorithm: {ZH_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

pub(crate) fn run_zh_artifact_bundle(
    options: ZhArtifactBundleCliOptions,
) -> Result<(), Box<dyn Error>> {
    let reading_options = validate_zh_dictionary_options(options.reading_options)?;
    let index =
        ZhReadingIndex::from_cedict_path_with_options(&options.cedict, options.index_options)?;
    let output_dir = PathBuf::from(&options.output_dir);
    let payload_file_name =
        default_zh_payload_file_name(&options.artifact_name, options.payload_format);
    let payload_path = output_dir.join(&payload_file_name);
    let metadata_path = output_dir.join("metadata.yaml");
    let license_output_dir = output_dir.join("license");

    fs::create_dir_all(&license_output_dir)?;
    write_zh_artifact_payload_file(&index, options.payload_format, &payload_path)?;
    let file_digest = zh_artifact_file_digest_path(&payload_path)?;

    let mut metadata = index.artifact_metadata(ZhArtifactMetadataOptions {
        artifact_name: options.artifact_name,
        generator: "moine-cli".to_string(),
        payload_file_name,
        payload_format: options.payload_format.as_str().to_string(),
        source_name: options.source_name,
        source_version: options.source_version,
        source_cedict: options.cedict.clone(),
        index_options: options.index_options,
        query_defaults: reading_options,
        license: ZhArtifactLicense::default(),
    });
    metadata.payload.file_digest_algorithm =
        Some(ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM.to_string());
    metadata.payload.file_digest = Some(file_digest.clone());
    fs::write(&metadata_path, serde_yaml::to_string(&metadata)?)?;

    write_zh_license_reference(options.license_file.as_deref(), &license_output_dir)?;

    println!("bundle: {}", output_dir.display());
    println!("metadata: {}", metadata_path.display());
    println!("payload: {}", payload_path.display());
    println!("payload_format: {}", options.payload_format.as_str());
    println!("pinyin_view: {}", index.pinyin_view().as_str());
    println!("entries: {}", index.len());
    println!("file_digest_algorithm: {ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
    println!("file_digest: {file_digest}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

pub(crate) fn run_zh_artifact_verify(
    options: ZhArtifactVerifyCliOptions,
) -> Result<(), Box<dyn Error>> {
    let verified = verify_zh_artifact_bundle(&options.metadata, options.bundle_dir.as_deref())?;

    println!("metadata: {}", verified.metadata_path.display());
    println!("bundle: {}", verified.bundle_dir.display());
    println!("payload: {}", verified.payload_path.display());
    println!("pinyin_view: {}", verified.index.pinyin_view().as_str());
    println!("entries: {}", verified.index.len());
    if let Some(file_digest) = &verified.file_digest {
        println!("file_digest_algorithm: {ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
        println!("file_digest: {file_digest}");
    } else {
        println!("file_digest: skipped");
    }
    println!(
        "checksum_algorithm: {}",
        verified.metadata.payload.checksum_algorithm
    );
    println!("checksum: {}", verified.checksum);
    println!(
        "license_references: {}",
        verified.metadata.license.references.len()
    );
    println!("verified: true");

    Ok(())
}

pub(crate) struct VerifiedZhArtifactBundle {
    pub(crate) metadata_path: PathBuf,
    pub(crate) bundle_dir: PathBuf,
    pub(crate) payload_path: PathBuf,
    pub(crate) metadata: ZhArtifactMetadata,
    pub(crate) index: ZhReadingIndex,
    pub(crate) file_digest: Option<String>,
    pub(crate) checksum: String,
}

pub(crate) fn write_zh_license_reference(
    license_file: Option<&str>,
    output_license_dir: &Path,
) -> Result<(), std::io::Error> {
    let output = output_license_dir.join("CC-CEDICT.md");
    if let Some(license_file) = license_file {
        fs::copy(license_file, output)?;
    } else {
        fs::write(
            output,
            "CC-CEDICT dictionary data is distributed under CC BY-SA 4.0.\n\
This moine artifact is derived from CC-CEDICT and keeps its license separate \
from the moine source-code license.\n",
        )?;
    }
    Ok(())
}

pub(crate) fn write_zh_artifact_payload_file(
    index: &ZhReadingIndex,
    payload_format: ArtifactPayloadFormat,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    match payload_format {
        ArtifactPayloadFormat::Yaml => {
            write_output_file(path, serde_yaml::to_string(&index.artifact_payload())?)?;
        }
        ArtifactPayloadFormat::Indexed => {
            let output = create_output_file(path)?;
            index.write_indexed_artifact_payload(output)?;
        }
        ArtifactPayloadFormat::Binary => {
            return Err(Box::new(CliError::ArtifactVerificationFailed(
                "binary zh payload format is not implemented; use yaml or indexed".to_string(),
            )));
        }
    }
    Ok(())
}

pub(crate) fn load_zh_index(
    source: &ZhIndexSource,
    index_options: CedictIndexOptions,
) -> Result<ZhReadingIndex, Box<dyn Error>> {
    match source {
        ZhIndexSource::Cedict(path) => Ok(ZhReadingIndex::from_cedict_path_with_options(
            path,
            index_options,
        )?),
        ZhIndexSource::ArtifactPayload {
            path,
            payload_format,
        } => load_zh_artifact_payload_by_format(Path::new(path), payload_format.as_str()),
        ZhIndexSource::ArtifactMetadata(path) => Ok(verify_zh_artifact_bundle(path, None)?.index),
    }
}

pub(crate) fn verify_zh_artifact_bundle(
    metadata: &str,
    bundle_dir: Option<&str>,
) -> Result<VerifiedZhArtifactBundle, Box<dyn Error>> {
    let metadata_path = PathBuf::from(metadata);
    let metadata_yaml = fs::read_to_string(&metadata_path)?;
    let metadata = serde_yaml::from_str::<ZhArtifactMetadata>(&metadata_yaml)?;
    validate_zh_dictionary_options(zh_dictionary_options_from_metadata(&metadata))?;
    if metadata.schema_version != 1 {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "unsupported zh metadata schema version {}",
            metadata.schema_version
        ))));
    }
    if metadata.artifact_type != "moine.zh.reading-index" {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "unsupported zh artifact type {:?}",
            metadata.artifact_type
        ))));
    }
    let bundle_dir = bundle_dir.map(PathBuf::from).unwrap_or_else(|| {
        metadata_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    let payload_path = checked_bundle_path(&bundle_dir, &metadata.payload.path)?;
    let file_digest = verify_zh_payload_file_digest(&metadata, &payload_path)?;
    let index = load_zh_artifact_payload_by_format(&payload_path, &metadata.payload.format)?;
    if index.len() != metadata.build.entries {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "entry count mismatch: metadata has {}, payload has {}",
            metadata.build.entries,
            index.len()
        ))));
    }
    if index.pinyin_view().as_str() != metadata.build.pinyin_view {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "pinyin view mismatch: metadata has {}, payload has {}",
            metadata.build.pinyin_view,
            index.pinyin_view().as_str()
        ))));
    }
    let checksum = index
        .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
        .ok_or_else(|| {
            CliError::ArtifactVerificationFailed(format!(
                "unsupported checksum algorithm {:?}",
                metadata.payload.checksum_algorithm
            ))
        })?;
    if checksum != metadata.payload.checksum {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "payload checksum mismatch: metadata has {}, recomputed {}",
            metadata.payload.checksum, checksum
        ))));
    }
    for reference in &metadata.license.references {
        let path = checked_bundle_path(&bundle_dir, &reference.path)?;
        if !path.is_file() {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "missing license reference {} at {}",
                reference.label,
                path.display()
            ))));
        }
    }

    Ok(VerifiedZhArtifactBundle {
        metadata_path,
        bundle_dir,
        payload_path,
        metadata,
        index,
        file_digest,
        checksum,
    })
}

fn zh_dictionary_options_from_metadata(metadata: &ZhArtifactMetadata) -> PinyinReadingOptions {
    PinyinReadingOptions {
        max_span_chars: metadata.query_defaults.max_span_chars,
        max_paths: metadata.query_defaults.max_paths,
        longest_match_only: metadata.query_defaults.longest_match_only,
        max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
    }
}

fn validate_zh_dictionary_options(
    options: PinyinReadingOptions,
) -> Result<PinyinReadingOptions, Box<dyn Error>> {
    options.validate().map_err(|err| {
        Box::new(CliError::ArtifactVerificationFailed(err.to_string())) as Box<dyn Error>
    })
}

pub(crate) fn verify_zh_payload_file_digest(
    metadata: &ZhArtifactMetadata,
    payload_path: &Path,
) -> Result<Option<String>, Box<dyn Error>> {
    match (
        metadata.payload.file_digest_algorithm.as_deref(),
        metadata.payload.file_digest.as_deref(),
    ) {
        (None, None) => Ok(None),
        (Some(algorithm), Some(expected)) => {
            if algorithm != ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM {
                return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                    "unsupported zh payload file digest algorithm {algorithm:?}"
                ))));
            }
            let digest = zh_artifact_file_digest_path(payload_path)?;
            if digest != expected {
                return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                    "payload file digest mismatch: metadata has {expected}, recomputed {digest}"
                ))));
            }
            Ok(Some(digest))
        }
        _ => Err(Box::new(CliError::ArtifactVerificationFailed(
            "zh payload file digest algorithm and digest must be provided together".to_string(),
        ))),
    }
}

pub(crate) fn load_zh_artifact_payload_by_format(
    path: &Path,
    payload_format: &str,
) -> Result<ZhReadingIndex, Box<dyn Error>> {
    match payload_format {
        YAML_PAYLOAD_FORMAT => Ok(ZhReadingIndex::from_artifact_payload_path(path)?),
        INDEXED_PAYLOAD_FORMAT => Ok(ZhReadingIndex::from_indexed_artifact_payload_path(path)?),
        _ => Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "unsupported zh payload format {payload_format:?}"
        )))),
    }
}

pub(crate) fn zh_release_archive_entries(
    verified: &VerifiedZhArtifactBundle,
) -> Result<Vec<ArchiveEntry>, Box<dyn Error>> {
    let mut entries = vec![
        ArchiveEntry {
            source: verified.metadata_path.clone(),
            path: "metadata.yaml".to_string(),
        },
        ArchiveEntry {
            source: verified.payload_path.clone(),
            path: normalized_relative_archive_path(&verified.metadata.payload.path)?,
        },
    ];
    for reference in &verified.metadata.license.references {
        entries.push(ArchiveEntry {
            source: checked_bundle_path(&verified.bundle_dir, &reference.path)?,
            path: normalized_relative_archive_path(&reference.path)?,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries.dedup_by(|left, right| left.path == right.path);
    Ok(entries)
}

pub(crate) fn run_zh_artifact_release_checksums(
    options: ZhArtifactReleaseChecksumsCliOptions,
) -> Result<(), Box<dyn Error>> {
    let mut output = String::new();

    for asset in &options.assets {
        let path = Path::new(asset);
        let digest = zh_artifact_file_digest_path(path)?;
        let label = release_checksum_asset_label(path)?;
        writeln!(output, "{digest}  {label}")?;
    }

    if let Some(path) = &options.output {
        write_output_file(path, output)?;
    } else {
        print!("{output}");
    }

    Ok(())
}
