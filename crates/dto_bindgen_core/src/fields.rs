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

    pub const fn optional_nullable_skip_if_none() -> Self {
        Self {
            nullable: true,
            required_on_deserialize: false,
            default: Some(DefaultKind::NoneValue),
            serialize_presence: SerializePresence::SkipIfNone,
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

    pub fn wire_contract(&self) -> FieldWireContract {
        FieldWireContract {
            nullable: self.nullable,
            required_on_deserialize: self.required_on_deserialize,
            default: self.default.clone(),
            serialize_presence: self.serialize_presence,
        }
    }

    pub fn contract(&self) -> FieldContract {
        FieldContract::from_wire_contract(&self.wire_contract())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldWireContract {
    pub nullable: bool,
    pub required_on_deserialize: bool,
    pub default: Option<DefaultKind>,
    pub serialize_presence: SerializePresence,
}

impl FieldWireContract {
    pub const fn is_serialized(&self) -> bool {
        !matches!(self.serialize_presence, SerializePresence::Never)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldContract {
    pub serialized: bool,
    pub required: bool,
    pub nullable: bool,
    pub default: Option<DefaultKind>,
    pub omit_when_none: bool,
}

impl FieldContract {
    pub fn from_wire_contract(wire: &FieldWireContract) -> Self {
        Self {
            serialized: wire.is_serialized(),
            required: wire.required_on_deserialize,
            nullable: wire.nullable,
            default: wire.default.clone(),
            omit_when_none: matches!(wire.serialize_presence, SerializePresence::SkipIfNone),
        }
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
    JsonNumber,
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
    fn optional_nullable_skip_if_none_presence_omits_none_on_serialize() {
        let presence = FieldPresence::optional_nullable_skip_if_none();
        assert!(presence.nullable);
        assert!(!presence.required_on_deserialize);
        assert_eq!(presence.default, Some(DefaultKind::NoneValue));
        assert_eq!(presence.serialize_presence, SerializePresence::SkipIfNone);
        assert!(presence.is_serialized());
    }

    #[test]
    fn skipped_presence_is_never_serialized() {
        let presence = FieldPresence::skipped();
        assert!(!presence.required_on_deserialize);
        assert!(!presence.is_serialized());
    }

    #[test]
    fn presence_projects_wire_and_exchange_contracts() {
        let presence = FieldPresence::optional_nullable_skip_if_none();

        let wire = presence.wire_contract();
        assert!(wire.nullable);
        assert!(!wire.required_on_deserialize);
        assert_eq!(wire.default, Some(DefaultKind::NoneValue));
        assert_eq!(wire.serialize_presence, SerializePresence::SkipIfNone);

        let contract = presence.contract();
        assert!(contract.serialized);
        assert!(!contract.required);
        assert!(contract.nullable);
        assert_eq!(contract.default, Some(DefaultKind::NoneValue));
        assert!(contract.omit_when_none);
    }

    #[test]
    fn exchange_contracts_cover_required_defaulted_and_skipped_fields() {
        let required = FieldPresence::required().contract();
        assert!(required.serialized);
        assert!(required.required);
        assert!(!required.nullable);
        assert_eq!(required.default, None);
        assert!(!required.omit_when_none);

        let defaulted = FieldPresence::defaulted(DefaultKind::EmptyVec).contract();
        assert!(defaulted.serialized);
        assert!(!defaulted.required);
        assert!(!defaulted.nullable);
        assert_eq!(defaulted.default, Some(DefaultKind::EmptyVec));
        assert!(!defaulted.omit_when_none);

        let skipped = FieldPresence::skipped().contract();
        assert!(!skipped.serialized);
        assert!(!skipped.required);
        assert!(!skipped.nullable);
        assert_eq!(skipped.default, None);
        assert!(!skipped.omit_when_none);
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
