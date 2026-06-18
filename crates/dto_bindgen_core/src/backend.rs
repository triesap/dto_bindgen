use core::fmt;
use core::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackendId {
    TypeScript,
    Python,
    Custom(String),
}

impl BackendId {
    pub const fn typescript() -> Self {
        Self::TypeScript
    }

    pub const fn python() -> Self {
        Self::Python
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Custom(value) => value.as_str(),
        }
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BackendId {
    type Err = ParseBackendIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "typescript" | "ts" => Ok(Self::TypeScript),
            "python" | "py" => Ok(Self::Python),
            "" => Err(ParseBackendIdError),
            other => Ok(Self::Custom(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseBackendIdError;

impl fmt::Display for ParseBackendIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("backend id cannot be empty")
    }
}

impl std::error::Error for ParseBackendIdError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_backend_ids() {
        assert_eq!("typescript".parse::<BackendId>(), Ok(BackendId::TypeScript));
        assert_eq!("py".parse::<BackendId>(), Ok(BackendId::Python));
    }

    #[test]
    fn preserves_custom_backend_ids() {
        let backend = "json_schema".parse::<BackendId>().unwrap();
        assert_eq!(backend.to_string(), "json_schema");
    }

    #[test]
    fn rejects_empty_backend_id() {
        assert!("".parse::<BackendId>().is_err());
    }
}
