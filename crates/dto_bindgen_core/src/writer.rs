use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{GeneratedFileSet, GeneratedManifest};

pub const DEFAULT_MANIFEST_NAME: &str = "dto_bindgen.generated.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputReport {
    pub files: Vec<PathBuf>,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckMismatch {
    pub path: PathBuf,
    pub kind: CheckMismatchKind,
}

impl CheckMismatch {
    fn new(path: PathBuf, kind: CheckMismatchKind) -> Self {
        Self { path, kind }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMismatchKind {
    Missing,
    Different,
}

#[derive(Debug)]
pub struct OutputWriter {
    root: PathBuf,
    manifest_name: String,
}

impl OutputWriter {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, OutputWriterError> {
        let original = root.as_ref().to_owned();
        let root = fs::canonicalize(&original).map_err(|source| OutputWriterError::Root {
            path: original.clone(),
            source,
        })?;

        if !root.is_dir() {
            return Err(OutputWriterError::RootNotDirectory { path: root });
        }

        Ok(Self {
            root,
            manifest_name: DEFAULT_MANIFEST_NAME.to_owned(),
        })
    }

    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join(&self.manifest_name)
    }

    pub fn write(
        &self,
        file_set: &GeneratedFileSet,
        manifest: &GeneratedManifest,
    ) -> Result<OutputReport, OutputWriterError> {
        let planned = self.plan_files(file_set)?;
        let manifest_json = self.render_manifest(manifest)?;
        let mut staged = Vec::new();

        for plan in &planned {
            self.ensure_parent_dir(&plan.target_path)?;
            let temp_path = self.temp_path(staged.len());
            fs::write(&temp_path, plan.contents.as_bytes()).map_err(|source| {
                OutputWriterError::WriteTemp {
                    path: temp_path.clone(),
                    source,
                }
            })?;
            staged.push(StagedWrite {
                temp_path,
                target_path: plan.target_path.clone(),
            });
        }

        let manifest_path = self.manifest_path();
        let temp_path = self.temp_path(staged.len());
        fs::write(&temp_path, manifest_json.as_bytes()).map_err(|source| {
            OutputWriterError::WriteTemp {
                path: temp_path.clone(),
                source,
            }
        })?;
        staged.push(StagedWrite {
            temp_path,
            target_path: manifest_path.clone(),
        });

        self.commit_staged(staged)?;

        Ok(OutputReport {
            files: planned.into_iter().map(|plan| plan.target_path).collect(),
            manifest_path,
        })
    }

    pub fn check(
        &self,
        file_set: &GeneratedFileSet,
        manifest: &GeneratedManifest,
    ) -> Result<OutputReport, OutputWriterError> {
        let planned = self.plan_files(file_set)?;
        let manifest_json = self.render_manifest(manifest)?;
        let manifest_path = self.manifest_path();
        let mut mismatches = Vec::new();

        for plan in &planned {
            push_mismatch_if_any(&mut mismatches, &plan.target_path, plan.contents.as_bytes())?;
        }

        push_mismatch_if_any(&mut mismatches, &manifest_path, manifest_json.as_bytes())?;

        if mismatches.is_empty() {
            Ok(OutputReport {
                files: planned.into_iter().map(|plan| plan.target_path).collect(),
                manifest_path,
            })
        } else {
            Err(OutputWriterError::CheckFailed { mismatches })
        }
    }

    fn plan_files(
        &self,
        file_set: &GeneratedFileSet,
    ) -> Result<Vec<PlannedFile>, OutputWriterError> {
        file_set
            .files()
            .iter()
            .map(|file| {
                let target_path = file.relative_path().join_to(&self.root);
                self.validate_target_path(&target_path)?;
                Ok(PlannedFile {
                    target_path,
                    contents: file.contents().to_owned(),
                })
            })
            .collect()
    }

    fn validate_target_path(&self, target_path: &Path) -> Result<(), OutputWriterError> {
        if !target_path.starts_with(&self.root) {
            return Err(OutputWriterError::PathEscapesRoot {
                path: target_path.to_owned(),
                root: self.root.clone(),
            });
        }

        let relative = target_path.strip_prefix(&self.root).map_err(|_| {
            OutputWriterError::PathEscapesRoot {
                path: target_path.to_owned(),
                root: self.root.clone(),
            }
        })?;

        let mut cursor = self.root.clone();
        for component in relative.components() {
            cursor.push(component.as_os_str());
            match fs::symlink_metadata(&cursor) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(OutputWriterError::SymlinkInOutputPath { path: cursor });
                }
                Ok(metadata) if cursor == target_path && metadata.is_dir() => {
                    return Err(OutputWriterError::TargetIsDirectory { path: cursor });
                }
                Ok(metadata) if cursor != target_path && !metadata.is_dir() => {
                    return Err(OutputWriterError::ParentIsNotDirectory { path: cursor });
                }
                Ok(_) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(OutputWriterError::ReadMetadata {
                        path: cursor,
                        source,
                    });
                }
            }
        }

        Ok(())
    }

    fn ensure_parent_dir(&self, target_path: &Path) -> Result<(), OutputWriterError> {
        let Some(parent) = target_path.parent() else {
            return Err(OutputWriterError::MissingParent {
                path: target_path.to_owned(),
            });
        };

        fs::create_dir_all(parent).map_err(|source| OutputWriterError::CreateDir {
            path: parent.to_owned(),
            source,
        })
    }

    fn render_manifest(&self, manifest: &GeneratedManifest) -> Result<String, OutputWriterError> {
        let mut json =
            serde_json::to_string_pretty(manifest).map_err(OutputWriterError::ManifestSerialize)?;
        json.push('\n');
        Ok(json)
    }

    fn commit_staged(&self, staged: Vec<StagedWrite>) -> Result<(), OutputWriterError> {
        let mut committed = Vec::new();

        for item in staged {
            let previous = PreviousTarget::capture(&item.target_path)?;
            match fs::rename(&item.temp_path, &item.target_path) {
                Ok(()) => committed.push((item.target_path, previous)),
                Err(source) => {
                    let rename_error = OutputWriterError::Rename {
                        from: item.temp_path.clone(),
                        to: item.target_path.clone(),
                        source,
                    };
                    let _ = fs::remove_file(&item.temp_path);
                    rollback(committed);
                    return Err(rename_error);
                }
            }
        }

        Ok(())
    }

    fn temp_path(&self, index: usize) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        self.root.join(format!(
            ".dto_bindgen.tmp.{}.{}.{}",
            std::process::id(),
            nonce,
            index
        ))
    }
}

#[derive(Debug)]
pub enum OutputWriterError {
    Root {
        path: PathBuf,
        source: std::io::Error,
    },
    RootNotDirectory {
        path: PathBuf,
    },
    PathEscapesRoot {
        path: PathBuf,
        root: PathBuf,
    },
    SymlinkInOutputPath {
        path: PathBuf,
    },
    TargetIsDirectory {
        path: PathBuf,
    },
    ParentIsNotDirectory {
        path: PathBuf,
    },
    MissingParent {
        path: PathBuf,
    },
    ReadMetadata {
        path: PathBuf,
        source: std::io::Error,
    },
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    WriteTemp {
        path: PathBuf,
        source: std::io::Error,
    },
    Rename {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    ManifestSerialize(serde_json::Error),
    CheckFailed {
        mismatches: Vec<CheckMismatch>,
    },
}

impl fmt::Display for OutputWriterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root { path, source } => {
                write!(
                    f,
                    "failed to canonicalize output root {}: {source}",
                    path.display()
                )
            }
            Self::RootNotDirectory { path } => {
                write!(f, "output root {} is not a directory", path.display())
            }
            Self::PathEscapesRoot { path, root } => write!(
                f,
                "generated path {} escapes output root {}",
                path.display(),
                root.display()
            ),
            Self::SymlinkInOutputPath { path } => {
                write!(f, "generated path {} crosses a symlink", path.display())
            }
            Self::TargetIsDirectory { path } => {
                write!(f, "generated target {} is a directory", path.display())
            }
            Self::ParentIsNotDirectory { path } => {
                write!(
                    f,
                    "generated target parent component {} is not a directory",
                    path.display()
                )
            }
            Self::MissingParent { path } => {
                write!(
                    f,
                    "generated target {} has no parent directory",
                    path.display()
                )
            }
            Self::ReadMetadata { path, source } => {
                write!(f, "failed to inspect {}: {source}", path.display())
            }
            Self::CreateDir { path, source } => {
                write!(f, "failed to create {}: {source}", path.display())
            }
            Self::WriteTemp { path, source } => {
                write!(f, "failed to write temp file {}: {source}", path.display())
            }
            Self::Rename { from, to, source } => write!(
                f,
                "failed to move temp file {} to {}: {source}",
                from.display(),
                to.display()
            ),
            Self::Read { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            Self::ManifestSerialize(source) => {
                write!(f, "failed to serialize generated manifest: {source}")
            }
            Self::CheckFailed { mismatches } => {
                write!(
                    f,
                    "generated output is stale ({} mismatch(es))",
                    mismatches.len()
                )
            }
        }
    }
}

impl std::error::Error for OutputWriterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Root { source, .. }
            | Self::ReadMetadata { source, .. }
            | Self::CreateDir { source, .. }
            | Self::WriteTemp { source, .. }
            | Self::Rename { source, .. }
            | Self::Read { source, .. } => Some(source),
            Self::ManifestSerialize(source) => Some(source),
            Self::RootNotDirectory { .. }
            | Self::PathEscapesRoot { .. }
            | Self::SymlinkInOutputPath { .. }
            | Self::TargetIsDirectory { .. }
            | Self::ParentIsNotDirectory { .. }
            | Self::MissingParent { .. }
            | Self::CheckFailed { .. } => None,
        }
    }
}

#[derive(Debug)]
struct PlannedFile {
    target_path: PathBuf,
    contents: String,
}

#[derive(Debug)]
struct StagedWrite {
    temp_path: PathBuf,
    target_path: PathBuf,
}

#[derive(Debug)]
enum PreviousTarget {
    Missing,
    Existing(Vec<u8>),
}

impl PreviousTarget {
    fn capture(path: &Path) -> Result<Self, OutputWriterError> {
        match fs::read(path) {
            Ok(bytes) => Ok(Self::Existing(bytes)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Self::Missing),
            Err(source) => Err(OutputWriterError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }
}

fn push_mismatch_if_any(
    mismatches: &mut Vec<CheckMismatch>,
    path: &Path,
    expected: &[u8],
) -> Result<(), OutputWriterError> {
    match fs::read(path) {
        Ok(actual) if actual == expected => Ok(()),
        Ok(_) => {
            mismatches.push(CheckMismatch::new(
                path.to_owned(),
                CheckMismatchKind::Different,
            ));
            Ok(())
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            mismatches.push(CheckMismatch::new(
                path.to_owned(),
                CheckMismatchKind::Missing,
            ));
            Ok(())
        }
        Err(source) => Err(OutputWriterError::Read {
            path: path.to_owned(),
            source,
        }),
    }
}

fn rollback(committed: Vec<(PathBuf, PreviousTarget)>) {
    for (path, previous) in committed.into_iter().rev() {
        match previous {
            PreviousTarget::Missing => {
                let _ = fs::remove_file(path);
            }
            PreviousTarget::Existing(bytes) => {
                let _ = fs::write(path, bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::{BackendId, GeneratedFile, GeneratedFileSet};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "dto_bindgen_writer_test_{}_{}",
                std::process::id(),
                counter
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            self.0.as_path()
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample_file_set() -> GeneratedFileSet {
        let file = GeneratedFile::new(
            BackendId::TypeScript,
            "generated/ts/user.ts",
            "export type User = { id: string };\n",
        )
        .unwrap();
        GeneratedFileSet::try_from_files([file]).unwrap()
    }

    fn sample_manifest(file_set: &GeneratedFileSet) -> GeneratedManifest {
        GeneratedManifest::from_file_set("0.1.0", "registry", "config", file_set)
    }

    #[test]
    fn writes_files_and_manifest_under_root() {
        let root = TempRoot::new();
        let writer = OutputWriter::new(root.path()).unwrap();
        let file_set = sample_file_set();
        let manifest = sample_manifest(&file_set);

        let report = writer.write(&file_set, &manifest).unwrap();

        assert_eq!(report.files.len(), 1);
        assert_eq!(
            fs::read_to_string(root.path().join("generated/ts/user.ts")).unwrap(),
            "export type User = { id: string };\n"
        );
        let manifest_json = fs::read_to_string(root.path().join(DEFAULT_MANIFEST_NAME)).unwrap();
        assert!(manifest_json.contains("\"generator\": \"dto_bindgen\""));
        assert!(manifest_json.contains("\"path\": \"generated/ts/user.ts\""));
    }

    #[test]
    fn check_passes_after_write() {
        let root = TempRoot::new();
        let writer = OutputWriter::new(root.path()).unwrap();
        let file_set = sample_file_set();
        let manifest = sample_manifest(&file_set);

        writer.write(&file_set, &manifest).unwrap();
        let report = writer.check(&file_set, &manifest).unwrap();

        assert_eq!(report.files.len(), 1);
        assert_eq!(
            report.manifest_path,
            root.path()
                .canonicalize()
                .unwrap()
                .join(DEFAULT_MANIFEST_NAME)
        );
    }

    #[test]
    fn check_reports_drift_without_writing() {
        let root = TempRoot::new();
        let writer = OutputWriter::new(root.path()).unwrap();
        let file_set = sample_file_set();
        let manifest = sample_manifest(&file_set);

        writer.write(&file_set, &manifest).unwrap();
        fs::write(root.path().join("generated/ts/user.ts"), "drift\n").unwrap();

        let err = writer.check(&file_set, &manifest).unwrap_err();

        let OutputWriterError::CheckFailed { mismatches } = err else {
            panic!("expected check failure");
        };
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].kind, CheckMismatchKind::Different);
        assert_eq!(
            fs::read_to_string(root.path().join("generated/ts/user.ts")).unwrap(),
            "drift\n"
        );
    }

    #[test]
    fn rejects_file_roots() {
        let root = TempRoot::new();
        let file_root = root.path().join("not_a_directory");
        fs::write(&file_root, "").unwrap();

        let err = OutputWriter::new(&file_root).unwrap_err();

        assert!(matches!(err, OutputWriterError::RootNotDirectory { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_output_path() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new();
        let outside = TempRoot::new();
        symlink(outside.path(), root.path().join("linked")).unwrap();
        let writer = OutputWriter::new(root.path()).unwrap();
        let file =
            GeneratedFile::new(BackendId::TypeScript, "linked/user.ts", "outside\n").unwrap();
        let file_set = GeneratedFileSet::try_from_files([file]).unwrap();
        let manifest = sample_manifest(&file_set);

        let err = writer.write(&file_set, &manifest).unwrap_err();

        assert!(matches!(err, OutputWriterError::SymlinkInOutputPath { .. }));
        assert!(!outside.path().join("user.ts").exists());
    }
}
