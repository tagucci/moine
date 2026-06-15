use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use moine_ja::{
    artifact_file_digest_path, compare_with_unidic_index, DictionaryReadingOptions,
    UnidicArtifactLicense, UnidicArtifactLicenseReference, UnidicArtifactMetadata,
    UnidicArtifactMetadataOptions, UnidicIndexOptions, UnidicReadingIndex,
    ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM, ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM,
};

use crate::archive::{
    checked_bundle_path, create_output_file, duration_ms, normalized_relative_archive_path,
    release_checksum_asset_label, write_output_file, write_release_archive, ArchiveEntry,
};
use crate::args::{
    default_unidic_license_dir, default_unidic_payload_file_name, max_readings_per_segment_label,
    ArtifactPayloadFormat, CliError, SudachiArtifactBundleCliOptions,
    UnidicArtifactArchiveCliOptions, UnidicArtifactBinaryInspectCliOptions,
    UnidicArtifactBinaryPayloadCliOptions, UnidicArtifactBundleCliOptions,
    UnidicArtifactInspectCliOptions, UnidicArtifactMetadataCliOptions,
    UnidicArtifactPayloadCliOptions, UnidicArtifactReleaseChecksumsCliOptions,
    UnidicArtifactRuntimeMeasureCliOptions, UnidicArtifactVerifyCliOptions, BINARY_PAYLOAD_FORMAT,
    INDEXED_PAYLOAD_FORMAT, YAML_PAYLOAD_FORMAT,
};

pub(crate) fn run_unidic_artifact_archive(
    options: UnidicArtifactArchiveCliOptions,
) -> Result<(), Box<dyn Error>> {
    let verified =
        verify_unidic_artifact_bundle(&options.metadata, options.bundle_dir.as_deref(), false)?;
    let root_name = options
        .root_name
        .unwrap_or_else(|| verified.metadata.artifact_name.clone());
    let entries = release_archive_entries(&verified)?;
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

pub(crate) fn run_unidic_artifact_metadata(
    options: UnidicArtifactMetadataCliOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let metadata = index.artifact_metadata(UnidicArtifactMetadataOptions {
        artifact_name: options.artifact_name,
        generator: "moine-cli".to_string(),
        payload_file_name: options.payload_file_name,
        payload_format: options.payload_format.as_str().to_string(),
        source_name: options.source_name,
        source_version: options.source_version,
        source_lex_csv: options.lex_csv.clone(),
        index_options: options.index_options,
        query_defaults: options.dictionary_options,
        license: UnidicArtifactLicense::default(),
    });
    let yaml = serde_yaml::to_string(&metadata)?;

    if let Some(output) = &options.output {
        write_output_file(output, yaml)?;
    } else {
        print!("{yaml}");
    }

    Ok(())
}

pub(crate) fn run_unidic_artifact_bundle(
    options: UnidicArtifactBundleCliOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let output_dir = PathBuf::from(&options.output_dir);
    let payload_file_name =
        default_unidic_payload_file_name(&options.artifact_name, options.payload_format);
    let payload_path = output_dir.join(&payload_file_name);
    let metadata_path = output_dir.join("metadata.yaml");
    let license_output_dir = output_dir.join("license");

    fs::create_dir_all(&license_output_dir)?;
    write_artifact_payload_file(&index, options.payload_format, &payload_path)?;
    let file_digest = artifact_file_digest_path(&payload_path)?;

    let mut metadata = index.artifact_metadata(UnidicArtifactMetadataOptions {
        artifact_name: options.artifact_name,
        generator: "moine-cli".to_string(),
        payload_file_name,
        payload_format: options.payload_format.as_str().to_string(),
        source_name: options.source_name,
        source_version: options.source_version,
        source_lex_csv: options.lex_csv.clone(),
        index_options: options.index_options,
        query_defaults: options.dictionary_options,
        license: UnidicArtifactLicense::default(),
    });
    metadata.payload.file_digest_algorithm =
        Some(ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM.to_string());
    metadata.payload.file_digest = Some(file_digest.clone());
    fs::write(&metadata_path, serde_yaml::to_string(&metadata)?)?;

    let license_dir = options
        .license_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| default_unidic_license_dir(&options.lex_csv));
    copy_unidic_license_file(&license_dir, &license_output_dir, "BSD")?;
    copy_unidic_license_file(&license_dir, &license_output_dir, "COPYING")?;

    println!("bundle: {}", output_dir.display());
    println!("metadata: {}", metadata_path.display());
    println!("payload: {}", payload_path.display());
    println!("payload_format: {}", options.payload_format.as_str());
    println!("entries: {}", index.len());
    println!("file_digest_algorithm: {ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
    println!("file_digest: {file_digest}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

pub(crate) fn run_sudachi_artifact_bundle(
    options: SudachiArtifactBundleCliOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_sudachi_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let output_dir = PathBuf::from(&options.output_dir);
    let payload_file_name =
        default_unidic_payload_file_name(&options.artifact_name, options.payload_format);
    let payload_path = output_dir.join(&payload_file_name);
    let metadata_path = output_dir.join("metadata.yaml");

    write_artifact_payload_file(&index, options.payload_format, &payload_path)?;
    let file_digest = artifact_file_digest_path(&payload_path)?;

    let license = sudachi_artifact_license();
    let mut metadata = index.artifact_metadata_with_build(
        UnidicArtifactMetadataOptions {
            artifact_name: options.artifact_name,
            generator: "moine-cli".to_string(),
            payload_file_name,
            payload_format: options.payload_format.as_str().to_string(),
            source_name: options.source_name,
            source_version: options.source_version,
            source_lex_csv: options.lex_csv.clone(),
            index_options: UnidicIndexOptions::default(),
            query_defaults: options.dictionary_options,
            license,
        },
        moine_ja::UnidicArtifactBuild {
            reading_field: "sudachi-reading".to_string(),
            max_readings_per_surface: options.index_options.max_readings_per_surface,
            exclude_ascii_surfaces: options.index_options.exclude_ascii_surfaces,
            exclude_symbol_pos: options.index_options.exclude_symbol_pos,
            include_normalized_surfaces: options.index_options.include_normalized_surfaces,
            exclude_unsupported_readings: options.index_options.exclude_unsupported_readings,
            entries: index.len(),
        },
    );
    metadata.payload.file_digest_algorithm =
        Some(ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM.to_string());
    metadata.payload.file_digest = Some(file_digest.clone());
    fs::write(&metadata_path, serde_yaml::to_string(&metadata)?)?;

    let license_output_dir = output_dir.join("license");
    fs::create_dir_all(&license_output_dir)?;
    fs::copy(
        &options.license_file,
        license_output_dir.join("LICENSE-2.0.txt"),
    )?;
    fs::copy(&options.legal_file, license_output_dir.join("LEGAL"))?;

    println!("bundle: {}", output_dir.display());
    println!("metadata: {}", metadata_path.display());
    println!("payload: {}", payload_path.display());
    println!("payload_format: {}", options.payload_format.as_str());
    println!("entries: {}", index.len());
    println!("file_digest_algorithm: {ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
    println!("file_digest: {file_digest}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

pub(crate) fn run_unidic_artifact_verify(
    options: UnidicArtifactVerifyCliOptions,
) -> Result<(), Box<dyn Error>> {
    let verified = verify_unidic_artifact_bundle(
        &options.metadata,
        options.bundle_dir.as_deref(),
        options.canonical_checksum,
    )?;

    println!("metadata: {}", verified.metadata_path.display());
    println!("bundle: {}", verified.bundle_dir.display());
    println!("payload: {}", verified.payload_path.display());
    println!("payload_format: {}", verified.metadata.payload.format);
    println!("entries: {}", verified.entries);
    if verified.used_binary_header {
        println!("entry_count_source: binary_header");
    } else {
        println!("entry_count_source: decoded_payload");
    }
    if let Some(file_digest) = &verified.file_digest {
        println!("file_digest_algorithm: {ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
        println!("file_digest: {file_digest}");
    }
    if let Some(checksum) = &verified.checksum {
        println!(
            "checksum_algorithm: {}",
            verified.metadata.payload.checksum_algorithm
        );
        println!("checksum: {checksum}");
    } else {
        println!("canonical_checksum: skipped");
    }
    println!(
        "license_references: {}",
        verified.metadata.license.references.len()
    );
    println!("verified: true");

    Ok(())
}

pub(crate) fn run_unidic_artifact_release_checksums(
    options: UnidicArtifactReleaseChecksumsCliOptions,
) -> Result<(), Box<dyn Error>> {
    let mut output = String::new();

    for asset in &options.assets {
        let path = Path::new(asset);
        let digest = artifact_file_digest_path(path)?;
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

pub(crate) fn run_unidic_artifact_runtime_measure(
    options: UnidicArtifactRuntimeMeasureCliOptions,
) -> Result<(), Box<dyn Error>> {
    let loaded =
        load_unidic_artifact_bundle_for_runtime(&options.metadata, options.bundle_dir.as_deref())?;
    let dictionary_options = dictionary_options_from_metadata(&loaded.metadata);

    let first_start = Instant::now();
    let first_distances = compare_with_unidic_index(
        &options.pairs[0].left,
        &options.pairs[0].right,
        &loaded.index,
        dictionary_options,
    )?;
    let first_compare = first_start.elapsed();

    let mut result_checksum = first_distances
        .surface_levenshtein
        .wrapping_add(first_distances.surface_damerau)
        .wrapping_add(first_distances.lattice)
        .wrapping_add(first_distances.lattice_damerau)
        .wrapping_add(first_distances.combined);
    for _ in 0..options.warmups {
        for pair in &options.pairs {
            let distances = compare_with_unidic_index(
                &pair.left,
                &pair.right,
                &loaded.index,
                dictionary_options,
            )?;
            result_checksum = result_checksum.wrapping_add(distances.combined);
        }
    }

    let mut compare_total = Duration::ZERO;
    let mut compare_min: Option<Duration> = None;
    let mut compare_max: Option<Duration> = None;
    for _ in 0..options.iterations {
        for pair in &options.pairs {
            let compare_start = Instant::now();
            let distances = compare_with_unidic_index(
                &pair.left,
                &pair.right,
                &loaded.index,
                dictionary_options,
            )?;
            let elapsed = compare_start.elapsed();
            compare_total += elapsed;
            compare_min = Some(compare_min.map_or(elapsed, |value| value.min(elapsed)));
            compare_max = Some(compare_max.map_or(elapsed, |value| value.max(elapsed)));
            result_checksum = result_checksum
                .wrapping_add(distances.surface_levenshtein)
                .wrapping_add(distances.surface_damerau)
                .wrapping_add(distances.lattice)
                .wrapping_add(distances.lattice_damerau)
                .wrapping_add(distances.combined);
        }
    }
    let measured_comparisons = options.iterations * options.pairs.len();
    let compare_avg = compare_total.as_secs_f64() * 1000.0 / measured_comparisons as f64;

    println!("metadata: {}", loaded.metadata_path.display());
    println!("bundle: {}", loaded.bundle_dir.display());
    println!("payload: {}", loaded.payload_path.display());
    println!("payload_format: {}", loaded.metadata.payload.format);
    println!("entries: {}", loaded.index.len());
    println!("file_digest_verified: {}", loaded.file_digest_verified);
    println!("query_defaults:");
    println!("  max_span_chars: {}", dictionary_options.max_span_chars);
    println!("  max_paths: {}", dictionary_options.max_paths);
    println!(
        "  longest_match_only: {}",
        dictionary_options.longest_match_only
    );
    println!(
        "  max_readings_per_segment: {}",
        max_readings_per_segment_label(dictionary_options.max_readings_per_segment)
    );
    println!(
        "timing_read_metadata_ms: {:.3}",
        duration_ms(loaded.timing.read_metadata)
    );
    println!(
        "timing_file_digest_ms: {:.3}",
        duration_ms(loaded.timing.file_digest)
    );
    println!(
        "timing_decode_payload_ms: {:.3}",
        duration_ms(loaded.timing.decode_payload)
    );
    if let Some(canonical_checksum) = loaded.timing.canonical_checksum {
        println!(
            "timing_canonical_checksum_ms: {:.3}",
            duration_ms(canonical_checksum)
        );
    }
    println!(
        "timing_load_total_ms: {:.3}",
        duration_ms(loaded.timing.total)
    );
    println!("pair_count: {}", options.pairs.len());
    println!("warmup_iterations: {}", options.warmups);
    println!("measure_iterations: {}", options.iterations);
    println!("measured_comparisons: {measured_comparisons}");
    println!("first_compare_ms: {:.3}", duration_ms(first_compare));
    println!("compare_total_ms: {:.3}", duration_ms(compare_total));
    println!("compare_avg_ms: {compare_avg:.3}");
    println!(
        "compare_min_ms: {:.3}",
        duration_ms(compare_min.expect("iterations should be non-zero"))
    );
    println!(
        "compare_max_ms: {:.3}",
        duration_ms(compare_max.expect("iterations should be non-zero"))
    );
    println!("result_checksum: {result_checksum}");
    println!("first_pair:");
    println!("  left: {}", options.pairs[0].left);
    println!("  right: {}", options.pairs[0].right);
    println!(
        "  surface_levenshtein: {}",
        first_distances.surface_levenshtein
    );
    println!("  surface_damerau: {}", first_distances.surface_damerau);
    println!("  lattice: {}", first_distances.lattice);
    println!("  lattice_damerau: {}", first_distances.lattice_damerau);
    println!("  combined: {}", first_distances.combined);

    Ok(())
}

pub(crate) fn run_unidic_artifact_binary_payload(
    options: UnidicArtifactBinaryPayloadCliOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let output = create_output_file(&options.output)?;
    index.write_artifact_binary_payload(output)?;

    println!("payload: {}", options.output);
    println!("format: binary.surface-readings.v1");
    println!("entries: {}", index.len());
    println!("checksum_algorithm: {ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

pub(crate) fn run_unidic_artifact_binary_inspect(
    options: UnidicArtifactBinaryInspectCliOptions,
) -> Result<(), Box<dyn Error>> {
    let (index, timing) = if options.timing {
        let read_start = Instant::now();
        let bytes = fs::read(&options.payload)?;
        let read_file = read_start.elapsed();

        let decode_start = Instant::now();
        let index = UnidicReadingIndex::from_binary_artifact_payload_reader(bytes.as_slice())?;
        let decode_binary = decode_start.elapsed();

        (
            index,
            Some(BinaryInspectTiming {
                read_file,
                decode_binary,
            }),
        )
    } else {
        (
            UnidicReadingIndex::from_binary_artifact_payload_path(&options.payload)?,
            None,
        )
    };

    let checksum_start = Instant::now();
    let checksum = index.artifact_payload_checksum();
    let checksum_elapsed = checksum_start.elapsed();

    println!("payload: {}", options.payload);
    println!("format: binary.surface-readings.v1");
    println!("entries: {}", index.len());
    println!("checksum_algorithm: {ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM}");
    println!("checksum: {checksum}");
    if let Some(timing) = timing {
        println!("timing_read_file_ms: {:.3}", duration_ms(timing.read_file));
        println!(
            "timing_decode_binary_ms: {:.3}",
            duration_ms(timing.decode_binary)
        );
        println!("timing_checksum_ms: {:.3}", duration_ms(checksum_elapsed));
    }

    Ok(())
}

pub(crate) fn run_unidic_artifact_inspect(
    options: UnidicArtifactInspectCliOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_artifact_payload_path(&options.payload)?;

    println!("payload: {}", options.payload);
    println!("entries: {}", index.len());
    println!("checksum_algorithm: {ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

pub(crate) fn run_unidic_artifact_payload(
    options: UnidicArtifactPayloadCliOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let yaml = serde_yaml::to_string(&index.artifact_payload())?;

    if let Some(output) = &options.output {
        write_output_file(output, yaml)?;
    } else {
        print!("{yaml}");
    }

    Ok(())
}

pub(crate) struct VerifiedArtifactBundle {
    pub(crate) metadata_path: PathBuf,
    pub(crate) bundle_dir: PathBuf,
    pub(crate) payload_path: PathBuf,
    pub(crate) metadata: UnidicArtifactMetadata,
    pub(crate) entries: usize,
    pub(crate) file_digest: Option<String>,
    pub(crate) checksum: Option<String>,
    pub(crate) used_binary_header: bool,
}

#[derive(Clone, Debug)]

pub(crate) struct BinaryInspectTiming {
    pub(crate) read_file: Duration,
    pub(crate) decode_binary: Duration,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RuntimeLoadTiming {
    pub(crate) read_metadata: Duration,
    pub(crate) file_digest: Duration,
    pub(crate) decode_payload: Duration,
    pub(crate) canonical_checksum: Option<Duration>,
    pub(crate) total: Duration,
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeLoadedArtifactBundle {
    pub(crate) metadata_path: PathBuf,
    pub(crate) bundle_dir: PathBuf,
    pub(crate) payload_path: PathBuf,
    pub(crate) metadata: UnidicArtifactMetadata,
    pub(crate) index: UnidicReadingIndex,
    pub(crate) file_digest_verified: bool,
    pub(crate) timing: RuntimeLoadTiming,
}

pub(crate) fn copy_unidic_license_file(
    license_dir: &Path,
    output_license_dir: &Path,
    file_name: &str,
) -> Result<(), std::io::Error> {
    fs::copy(
        license_dir.join(file_name),
        output_license_dir.join(file_name),
    )?;
    Ok(())
}

pub(crate) fn sudachi_artifact_license() -> UnidicArtifactLicense {
    UnidicArtifactLicense {
        selected_license: "Apache-2.0".to_string(),
        references: vec![
            UnidicArtifactLicenseReference {
                label: "LICENSE-2.0.txt".to_string(),
                path: "license/LICENSE-2.0.txt".to_string(),
            },
            UnidicArtifactLicenseReference {
                label: "LEGAL".to_string(),
                path: "license/LEGAL".to_string(),
            },
        ],
    }
}

pub(crate) fn write_artifact_payload_file(
    index: &UnidicReadingIndex,
    payload_format: ArtifactPayloadFormat,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    match payload_format {
        ArtifactPayloadFormat::Yaml => {
            write_output_file(path, serde_yaml::to_string(&index.artifact_payload())?)?;
        }
        ArtifactPayloadFormat::Binary => {
            let output = create_output_file(path)?;
            index.write_artifact_binary_payload(output)?;
        }
        ArtifactPayloadFormat::Indexed => {
            let output = create_output_file(path)?;
            index.write_indexed_artifact_payload(output)?;
        }
    }
    Ok(())
}

pub(crate) fn load_artifact_payload_by_format(
    path: &Path,
    payload_format: &str,
) -> Result<UnidicReadingIndex, Box<dyn Error>> {
    match payload_format {
        YAML_PAYLOAD_FORMAT => Ok(UnidicReadingIndex::from_artifact_payload_path(path)?),
        BINARY_PAYLOAD_FORMAT => Ok(UnidicReadingIndex::from_binary_artifact_payload_path(path)?),
        INDEXED_PAYLOAD_FORMAT => Ok(UnidicReadingIndex::from_indexed_artifact_payload_path(
            path,
        )?),
        _ => Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "unsupported payload format {payload_format:?}"
        )))),
    }
}

pub(crate) fn verify_unidic_artifact_bundle(
    metadata: &str,
    bundle_dir: Option<&str>,
    _force_canonical_checksum: bool,
) -> Result<VerifiedArtifactBundle, Box<dyn Error>> {
    let metadata_path = PathBuf::from(metadata);
    let metadata_yaml = fs::read_to_string(&metadata_path)?;
    let metadata = serde_yaml::from_str::<UnidicArtifactMetadata>(&metadata_yaml)?;
    let bundle_dir = bundle_dir.map(PathBuf::from).unwrap_or_else(|| {
        metadata_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    let payload_path = checked_bundle_path(&bundle_dir, &metadata.payload.path)?;

    let file_digest = verify_payload_file_digest(&metadata, &payload_path)?;
    let (entries, checksum, used_binary_header) = {
        let index = load_artifact_payload_by_format(&payload_path, &metadata.payload.format)?;
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
        (index.len(), Some(checksum), false)
    };
    if entries != metadata.build.entries {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "entry count mismatch: metadata has {}, payload has {}",
            metadata.build.entries, entries
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

    Ok(VerifiedArtifactBundle {
        metadata_path,
        bundle_dir,
        payload_path,
        entries,
        file_digest,
        checksum,
        used_binary_header,
        metadata,
    })
}

pub(crate) fn load_unidic_artifact_bundle_for_runtime(
    metadata: &str,
    bundle_dir: Option<&str>,
) -> Result<RuntimeLoadedArtifactBundle, Box<dyn Error>> {
    let load_start = Instant::now();
    let metadata_path = PathBuf::from(metadata);

    let read_metadata_start = Instant::now();
    let metadata_yaml = fs::read_to_string(&metadata_path)?;
    let metadata = serde_yaml::from_str::<UnidicArtifactMetadata>(&metadata_yaml)?;
    let read_metadata = read_metadata_start.elapsed();

    let bundle_dir = bundle_dir.map(PathBuf::from).unwrap_or_else(|| {
        metadata_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    let payload_path = checked_bundle_path(&bundle_dir, &metadata.payload.path)?;

    let file_digest_start = Instant::now();
    let file_digest = verify_payload_file_digest(&metadata, &payload_path)?;
    let file_digest_elapsed = file_digest_start.elapsed();

    let decode_start = Instant::now();
    let index = load_artifact_payload_by_format(&payload_path, &metadata.payload.format)?;
    let decode_payload = decode_start.elapsed();

    let checksum_start = Instant::now();
    verify_loaded_artifact_payload_checksum(&metadata, &index)?;
    let canonical_checksum = Some(checksum_start.elapsed());

    if index.len() != metadata.build.entries {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "entry count mismatch: metadata has {}, payload has {}",
            metadata.build.entries,
            index.len()
        ))));
    }

    Ok(RuntimeLoadedArtifactBundle {
        metadata_path,
        bundle_dir,
        payload_path,
        file_digest_verified: file_digest.is_some(),
        timing: RuntimeLoadTiming {
            read_metadata,
            file_digest: file_digest_elapsed,
            decode_payload,
            canonical_checksum,
            total: load_start.elapsed(),
        },
        metadata,
        index,
    })
}

pub(crate) fn verify_loaded_artifact_payload_checksum(
    metadata: &UnidicArtifactMetadata,
    index: &UnidicReadingIndex,
) -> Result<(), Box<dyn Error>> {
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
    Ok(())
}

pub(crate) fn dictionary_options_from_metadata(
    metadata: &UnidicArtifactMetadata,
) -> DictionaryReadingOptions {
    DictionaryReadingOptions {
        max_span_chars: metadata.query_defaults.max_span_chars,
        max_paths: metadata.query_defaults.max_paths,
        longest_match_only: metadata.query_defaults.longest_match_only,
        max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
    }
}

pub(crate) fn verify_payload_file_digest(
    metadata: &UnidicArtifactMetadata,
    payload_path: &Path,
) -> Result<Option<String>, Box<dyn Error>> {
    match (
        metadata.payload.file_digest_algorithm.as_deref(),
        metadata.payload.file_digest.as_deref(),
    ) {
        (None, None) => Ok(None),
        (Some(algorithm), Some(expected)) => {
            if algorithm != ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM {
                return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                    "unsupported payload file digest algorithm {algorithm:?}"
                ))));
            }
            let digest = artifact_file_digest_path(payload_path)?;
            if digest != expected {
                return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                    "payload file digest mismatch: metadata has {expected}, recomputed {digest}"
                ))));
            }
            Ok(Some(digest))
        }
        _ => Err(Box::new(CliError::ArtifactVerificationFailed(
            "payload file digest algorithm and digest must be provided together".to_string(),
        ))),
    }
}

pub(crate) fn release_archive_entries(
    verified: &VerifiedArtifactBundle,
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
