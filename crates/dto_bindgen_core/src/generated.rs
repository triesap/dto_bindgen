use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{BackendId, GeneratedFileId};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeneratedRelativePath(String);

impl GeneratedRelativePath {
    pub fn new(path: impl AsRef<str>) -> Result<Self, GeneratedPathError> {
        let path = path.as_ref();

        if path.is_empty() {
            return Err(GeneratedPathError::Empty);
        }

        if path.contains('\\') {
            return Err(GeneratedPathError::Backslash {
                path: path.to_owned(),
            });
        }

        if has_windows_prefix(path) || Path::new(path).is_absolute() {
            return Err(GeneratedPathError::Absolute {
                path: path.to_owned(),
            });
        }

        let mut parts = Vec::new();
        for part in path.split('/') {
            match part {
                "" => {
                    return Err(GeneratedPathError::EmptyComponent {
                        path: path.to_owned(),
                    });
                }
                "." => {
                    return Err(GeneratedPathError::CurrentComponent {
                        path: path.to_owned(),
                    });
                }
                ".." => {
                    return Err(GeneratedPathError::ParentComponent {
                        path: path.to_owned(),
                    });
                }
                value => parts.push(value),
            }
        }

        Ok(Self(parts.join("/")))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn join_to(&self, root: impl AsRef<Path>) -> PathBuf {
        root.as_ref().join(self.as_str())
    }
}

impl fmt::Display for GeneratedRelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedPathError {
    Empty,
    EmptyComponent { path: String },
    CurrentComponent { path: String },
    ParentComponent { path: String },
    Absolute { path: String },
    Backslash { path: String },
}

impl fmt::Display for GeneratedPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("generated path cannot be empty"),
            Self::EmptyComponent { path } => {
                write!(f, "generated path `{path}` contains an empty component")
            }
            Self::CurrentComponent { path } => {
                write!(
                    f,
                    "generated path `{path}` contains a current-directory component"
                )
            }
            Self::ParentComponent { path } => {
                write!(
                    f,
                    "generated path `{path}` cannot contain parent-directory components"
                )
            }
            Self::Absolute { path } => {
                write!(f, "generated path `{path}` must be relative")
            }
            Self::Backslash { path } => {
                write!(f, "generated path `{path}` must use forward slashes")
            }
        }
    }
}

impl std::error::Error for GeneratedPathError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    backend: BackendId,
    relative_path: GeneratedRelativePath,
    contents: String,
}

impl GeneratedFile {
    pub fn new(
        backend: BackendId,
        relative_path: impl AsRef<str>,
        contents: impl Into<String>,
    ) -> Result<Self, GeneratedPathError> {
        Ok(Self {
            backend,
            relative_path: GeneratedRelativePath::new(relative_path)?,
            contents: contents.into(),
        })
    }

    pub fn backend(&self) -> &BackendId {
        &self.backend
    }

    pub fn relative_path(&self) -> &GeneratedRelativePath {
        &self.relative_path
    }

    pub fn contents(&self) -> &str {
        self.contents.as_str()
    }

    pub fn id(&self) -> GeneratedFileId {
        GeneratedFileId::new(self.backend.clone(), self.relative_path.as_str())
    }

    pub fn sha256(&self) -> String {
        sha256_hex(self.contents.as_bytes())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratedFileSet {
    files: Vec<GeneratedFile>,
}

impl GeneratedFileSet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn try_from_files(
        files: impl IntoIterator<Item = GeneratedFile>,
    ) -> Result<Self, GeneratedFileSetError> {
        let mut files = files.into_iter().collect::<Vec<_>>();
        files.sort_by(|left, right| {
            left.relative_path
                .cmp(&right.relative_path)
                .then_with(|| left.backend.cmp(&right.backend))
        });

        let mut seen = BTreeMap::<GeneratedRelativePath, BackendId>::new();
        for file in &files {
            if let Some(first_backend) =
                seen.insert(file.relative_path.clone(), file.backend.clone())
            {
                return Err(GeneratedFileSetError::DuplicatePath {
                    path: file.relative_path.clone(),
                    first_backend,
                    duplicate_backend: file.backend.clone(),
                });
            }
        }

        Ok(Self { files })
    }

    pub fn files(&self) -> &[GeneratedFile] {
        self.files.as_slice()
    }

    pub fn into_files(self) -> Vec<GeneratedFile> {
        self.files
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedFileSetError {
    DuplicatePath {
        path: GeneratedRelativePath,
        first_backend: BackendId,
        duplicate_backend: BackendId,
    },
}

impl fmt::Display for GeneratedFileSetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePath {
                path,
                first_backend,
                duplicate_backend,
            } => write!(
                f,
                "generated path `{path}` is produced by both {first_backend} and {duplicate_backend}"
            ),
        }
    }
}

impl std::error::Error for GeneratedFileSetError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedManifest {
    pub generator: String,
    pub schema_version: u32,
    pub version: String,
    pub registry_hash: String,
    pub config_hash: String,
    pub files: Vec<GeneratedManifestFile>,
}

impl GeneratedManifest {
    pub fn from_file_set(
        version: impl Into<String>,
        registry_hash: impl Into<String>,
        config_hash: impl Into<String>,
        file_set: &GeneratedFileSet,
    ) -> Self {
        Self {
            generator: "dto_bindgen".to_owned(),
            schema_version: crate::CONFIG_SCHEMA_VERSION,
            version: version.into(),
            registry_hash: registry_hash.into(),
            config_hash: config_hash.into(),
            files: file_set
                .files()
                .iter()
                .map(GeneratedManifestFile::from_generated_file)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedManifestFile {
    pub backend: String,
    pub path: String,
    pub sha256: String,
}

impl GeneratedManifestFile {
    pub fn from_generated_file(file: &GeneratedFile) -> Self {
        Self {
            backend: file.backend().as_str().to_owned(),
            path: file.relative_path().as_str().to_owned(),
            sha256: file.sha256(),
        }
    }
}

fn has_windows_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_safe_generated_paths() {
        let path = GeneratedRelativePath::new("generated/ts/user_profile.ts").unwrap();
        assert_eq!(path.as_str(), "generated/ts/user_profile.ts");
        assert_eq!(
            path.join_to("/tmp/out"),
            PathBuf::from("/tmp/out/generated/ts/user_profile.ts")
        );
    }

    #[test]
    fn rejects_unsafe_generated_paths() {
        assert_eq!(
            GeneratedRelativePath::new("").unwrap_err(),
            GeneratedPathError::Empty
        );
        assert!(matches!(
            GeneratedRelativePath::new("/tmp/out.ts").unwrap_err(),
            GeneratedPathError::Absolute { .. }
        ));
        assert!(matches!(
            GeneratedRelativePath::new("C:/out.ts").unwrap_err(),
            GeneratedPathError::Absolute { .. }
        ));
        assert!(matches!(
            GeneratedRelativePath::new("generated/../out.ts").unwrap_err(),
            GeneratedPathError::ParentComponent { .. }
        ));
        assert!(matches!(
            GeneratedRelativePath::new("generated//out.ts").unwrap_err(),
            GeneratedPathError::EmptyComponent { .. }
        ));
        assert!(matches!(
            GeneratedRelativePath::new("generated\\out.ts").unwrap_err(),
            GeneratedPathError::Backslash { .. }
        ));
    }

    #[test]
    fn generated_file_exposes_id_and_digest() {
        let file = GeneratedFile::new(BackendId::TypeScript, "hello.ts", "hello").unwrap();

        assert_eq!(
            file.id(),
            GeneratedFileId::new(BackendId::TypeScript, "hello.ts")
        );
        assert_eq!(
            file.sha256(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn file_set_sorts_paths_deterministically() {
        let zed = GeneratedFile::new(BackendId::Python, "z.py", "").unwrap();
        let alpha = GeneratedFile::new(BackendId::TypeScript, "a.ts", "").unwrap();

        let set = GeneratedFileSet::try_from_files([zed, alpha]).unwrap();

        assert_eq!(set.files()[0].relative_path().as_str(), "a.ts");
        assert_eq!(set.files()[1].relative_path().as_str(), "z.py");
    }

    #[test]
    fn file_set_rejects_duplicate_paths() {
        let first = GeneratedFile::new(BackendId::TypeScript, "shared/model", "ts").unwrap();
        let second = GeneratedFile::new(BackendId::Python, "shared/model", "py").unwrap();

        let err = GeneratedFileSet::try_from_files([first, second]).unwrap_err();

        assert!(matches!(
            err,
            GeneratedFileSetError::DuplicatePath {
                first_backend: BackendId::TypeScript,
                duplicate_backend: BackendId::Python,
                ..
            }
        ));
    }

    #[test]
    fn manifest_records_file_metadata() {
        let file =
            GeneratedFile::new(BackendId::TypeScript, "generated/ts/user.ts", "hello").unwrap();
        let set = GeneratedFileSet::try_from_files([file]).unwrap();
        let manifest =
            GeneratedManifest::from_file_set("0.1.0", "registry-hash", "config-hash", &set);

        assert_eq!(manifest.generator, "dto_bindgen");
        assert_eq!(manifest.schema_version, crate::CONFIG_SCHEMA_VERSION);
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.registry_hash, "registry-hash");
        assert_eq!(manifest.config_hash, "config-hash");
        assert_eq!(manifest.files[0].backend, "typescript");
        assert_eq!(manifest.files[0].path, "generated/ts/user.ts");
    }
}
