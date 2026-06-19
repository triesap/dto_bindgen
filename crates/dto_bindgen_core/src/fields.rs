use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdentName(String);

impl IdentName {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for IdentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocString(String);

impl DocString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WireFieldNames {
    pub serialize_name: String,
    pub deserialize_name: String,
    pub aliases: Vec<String>,
}

impl WireFieldNames {
    pub fn same(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            serialize_name: name.clone(),
            deserialize_name: name,
            aliases: Vec::new(),
        }
    }

    pub fn diverges(&self) -> bool {
        self.serialize_name != self.deserialize_name
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetFieldNames {
    pub typescript: String,
    pub python: String,
}

impl TargetFieldNames {
    pub fn new(typescript: impl Into<String>, python: impl Into<String>) -> Self {
        Self {
            typescript: typescript.into(),
            python: python.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldPresence {
    pub nullable: bool,
    pub required_on_deserialize: bool,
    pub default: Option<DefaultKind>,
    pub serialize_presence: SerializePresence,
}

impl FieldPresence {
    pub const fn required() -> Self {
        Self {
            nullable: false,
            required_on_deserialize: true,
            default: None,
            serialize_presence: SerializePresence::Always,
        }
    }

    pub const fn nullable_required() -> Self {
        Self {
            nullable: true,
            required_on_deserialize: true,
            default: None,
            serialize_presence: SerializePresence::Always,
        }
    }

    pub const fn optional_nullable() -> Self {
        Self {
            nullable: true,
            required_on_deserialize: false,
            default: Some(DefaultKind::NoneValue),
            serialize_presence: SerializePresence::Always,
        }
    }

    pub const fn defaulted(default: DefaultKind) -> Self {
        Self {
            nullable: false,
            required_on_deserialize: false,
            default: Some(default),
            serialize_presence: SerializePresence::Always,
        }
    }

    pub const fn skipped() -> Self {
        Self {
            nullable: false,
            required_on_deserialize: false,
            default: None,
            serialize_presence: SerializePresence::Never,
        }
    }

    pub const fn is_serialized(&self) -> bool {
        !matches!(self.serialize_presence, SerializePresence::Never)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefaultKind {
    NoneValue,
    EmptyString,
    EmptyVec,
    EmptyMap,
    BoolFalse,
    NumericZero,
    CustomPath(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntRepr {
    JsonString,
    JsonNumberUnsafe,
    NonJsonBigint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SerializePresence {
    Always,
    SkipIfNone,
    SkipIfDefault,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlattenMode {
    None,
    Flattened,
}

impl FlattenMode {
    pub const fn is_flattened(self) -> bool {
        matches!(self, Self::Flattened)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_presence_is_non_nullable_and_required() {
        let presence = FieldPresence::required();
        assert!(!presence.nullable);
        assert!(presence.required_on_deserialize);
        assert_eq!(presence.default, None);
        assert_eq!(presence.serialize_presence, SerializePresence::Always);
    }

    #[test]
    fn defaulted_presence_is_not_required_on_deserialize() {
        let presence = FieldPresence::defaulted(DefaultKind::EmptyVec);
        assert!(!presence.required_on_deserialize);
        assert_eq!(presence.default, Some(DefaultKind::EmptyVec));
        assert!(presence.is_serialized());
    }

    #[test]
    fn optional_nullable_presence_allows_missing_nulls() {
        let presence = FieldPresence::optional_nullable();
        assert!(presence.nullable);
        assert!(!presence.required_on_deserialize);
        assert_eq!(presence.default, Some(DefaultKind::NoneValue));
        assert_eq!(presence.serialize_presence, SerializePresence::Always);
    }

    #[test]
    fn skipped_presence_is_never_serialized() {
        let presence = FieldPresence::skipped();
        assert!(!presence.required_on_deserialize);
        assert!(!presence.is_serialized());
    }

    #[test]
    fn wire_names_can_detect_divergence() {
        let same = WireFieldNames::same("userId");
        assert!(!same.diverges());

        let divergent = WireFieldNames {
            serialize_name: "userId".to_owned(),
            deserialize_name: "user_id".to_owned(),
            aliases: Vec::new(),
        };
        assert!(divergent.diverges());
    }

    #[test]
    fn flatten_mode_is_explicit() {
        assert!(!FlattenMode::None.is_flattened());
        assert!(FlattenMode::Flattened.is_flattened());
    }
}
