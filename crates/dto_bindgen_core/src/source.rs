use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceFile {
    path: String,
}

impl SourceFile {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &str {
        self.path.as_str()
    }
}

impl fmt::Display for SourceFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.path())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourcePosition {
    pub line: u32,
    pub column: u32,
}

impl SourcePosition {
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

impl fmt::Display for SourcePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceSpan {
    pub file: SourceFile,
    pub start: SourcePosition,
    pub end: Option<SourcePosition>,
}

impl SourceSpan {
    pub fn new(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            file: SourceFile::new(file),
            start: SourcePosition::new(line, column),
            end: None,
        }
    }

    pub const fn with_end(mut self, line: u32, column: u32) -> Self {
        self.end = Some(SourcePosition::new(line, column));
        self
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.end {
            Some(end) => write!(f, "{}:{}-{}", self.file, self.start, end),
            None => write!(f, "{}:{}", self.file, self.start),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_single_position_span() {
        let span = SourceSpan::new("src/types.rs", 42, 5);
        assert_eq!(span.to_string(), "src/types.rs:42:5");
    }

    #[test]
    fn formats_range_span() {
        let span = SourceSpan::new("src/types.rs", 42, 5).with_end(42, 12);
        assert_eq!(span.to_string(), "src/types.rs:42:5-42:12");
    }
}
