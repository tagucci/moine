pub(crate) mod compare;
pub(crate) mod download;
pub(crate) mod unidic_artifact;
pub(crate) mod zh_artifact;

use std::error::Error;

use crate::args::{Cli, CliAction};

pub(crate) fn run_from_env() -> Result<(), Box<dyn Error>> {
    run_action(Cli::from_env().into_action()?)
}

pub(crate) fn run_with_args<I, S>(args: I) -> Result<(), Box<dyn Error>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    run_action(Cli::parse_from_args(args)?.into_action()?)
}

fn run_action(action: CliAction) -> Result<(), Box<dyn Error>> {
    match action {
        CliAction::CedictReadings(options) => compare::run_cedict_readings(options),
        CliAction::CedictSequences(options) => compare::run_cedict_sequences(options),
        CliAction::ChineseCompare(options) => compare::run_chinese_compare(options),
        CliAction::Compare(options) => compare::run_compare(options),
        CliAction::Download(options) => download::run_download(options),
        CliAction::List(options) => download::run_download_list(options),
        CliAction::Where(options) => download::run_download_where(options),
        CliAction::SudachiArtifactBundle(options) => {
            unidic_artifact::run_sudachi_artifact_bundle(options)
        }
        CliAction::SudachiCsvReadings(options) => compare::run_sudachi_csv_readings(options),
        CliAction::SudachiCsvSequences(options) => compare::run_sudachi_csv_sequences(options),
        CliAction::ZhArtifactArchive(options) => zh_artifact::run_zh_artifact_archive(options),
        CliAction::ZhArtifactBundle(options) => zh_artifact::run_zh_artifact_bundle(options),
        CliAction::ZhArtifactInspect(options) => zh_artifact::run_zh_artifact_inspect(options),
        CliAction::ZhArtifactMetadata(options) => zh_artifact::run_zh_artifact_metadata(options),
        CliAction::ZhArtifactPayload(options) => zh_artifact::run_zh_artifact_payload(options),
        CliAction::ZhArtifactReleaseChecksums(options) => {
            zh_artifact::run_zh_artifact_release_checksums(options)
        }
        CliAction::ZhArtifactVerify(options) => zh_artifact::run_zh_artifact_verify(options),
        CliAction::UnidicArtifactArchive(options) => {
            unidic_artifact::run_unidic_artifact_archive(options)
        }
        CliAction::UnidicArtifactBinaryInspect(options) => {
            unidic_artifact::run_unidic_artifact_binary_inspect(options)
        }
        CliAction::UnidicArtifactBinaryPayload(options) => {
            unidic_artifact::run_unidic_artifact_binary_payload(options)
        }
        CliAction::UnidicArtifactBundle(options) => {
            unidic_artifact::run_unidic_artifact_bundle(options)
        }
        CliAction::UnidicArtifactInspect(options) => {
            unidic_artifact::run_unidic_artifact_inspect(options)
        }
        CliAction::UnidicArtifactMetadata(options) => {
            unidic_artifact::run_unidic_artifact_metadata(options)
        }
        CliAction::UnidicArtifactPayload(options) => {
            unidic_artifact::run_unidic_artifact_payload(options)
        }
        CliAction::UnidicArtifactReleaseChecksums(options) => {
            unidic_artifact::run_unidic_artifact_release_checksums(options)
        }
        CliAction::UnidicArtifactRuntimeMeasure(options) => {
            unidic_artifact::run_unidic_artifact_runtime_measure(options)
        }
        CliAction::UnidicArtifactVerify(options) => {
            unidic_artifact::run_unidic_artifact_verify(options)
        }
        CliAction::UnidicCsvReadings(options) => compare::run_unidic_csv_readings(options),
        CliAction::UnidicCsvSequences(options) => compare::run_unidic_csv_sequences(options),
        CliAction::UnidicReadings(options) => compare::run_unidic_readings(options),
    }
}
