use crate::TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Primitive {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    I128,
    U128,
    Isize,
    Usize,
    F32,
    F64,
}

impl Primitive {
    pub const fn is_integer(self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::U8
                | Self::I16
                | Self::U16
                | Self::I32
                | Self::U32
                | Self::I64
                | Self::U64
                | Self::I128
                | Self::U128
                | Self::Isize
                | Self::Usize
        )
    }

    pub const fn requires_explicit_integer_policy(self) -> bool {
        matches!(
            self,
            Self::I64 | Self::U64 | Self::I128 | Self::U128 | Self::Isize | Self::Usize
        )
    }

    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BytesRepr {
    Bytes,
    Base64String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetOverride {
    pub backend: crate::BackendId,
    pub target_type: String,
}

impl TargetOverride {
    pub fn new(backend: crate::BackendId, target_type: impl Into<String>) -> Self {
        Self {
            backend,
            target_type: target_type.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeRef {
    Primitive(Primitive),
    String,
    Bytes(BytesRepr),
    Option(Box<TypeRef>),
    Vec(Box<TypeRef>),
    Array {
        item: Box<TypeRef>,
        len: usize,
    },
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
    },
    Named(TypeId),
    GenericParam(String),
    Override(TargetOverride),
}

impl TypeRef {
    pub fn option(inner: Self) -> Self {
        Self::Option(Box::new(inner))
    }

    pub fn vec(inner: Self) -> Self {
        Self::Vec(Box::new(inner))
    }

    pub fn array(item: Self, len: usize) -> Self {
        Self::Array {
            item: Box::new(item),
            len,
        }
    }

    pub fn string_keyed_map(value: Self) -> Self {
        Self::Map {
            key: Box::new(Self::String),
            value: Box::new(value),
        }
    }

    pub const fn named(type_id: TypeId) -> Self {
        Self::Named(type_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_large_integers_as_policy_requiring() {
        assert!(Primitive::I64.requires_explicit_integer_policy());
        assert!(Primitive::U128.requires_explicit_integer_policy());
        assert!(Primitive::Usize.requires_explicit_integer_policy());
        assert!(!Primitive::I32.requires_explicit_integer_policy());
        assert!(!Primitive::U32.requires_explicit_integer_policy());
    }

    #[test]
    fn classifies_float_primitives() {
        assert!(Primitive::F32.is_float());
        assert!(Primitive::F64.is_float());
        assert!(!Primitive::Bool.is_float());
    }

    #[test]
    fn builds_nested_type_refs() {
        let ty = TypeRef::option(TypeRef::vec(TypeRef::String));
        assert!(matches!(ty, TypeRef::Option(_)));
    }

    #[test]
    fn builds_string_keyed_maps() {
        let ty = TypeRef::string_keyed_map(TypeRef::Primitive(Primitive::Bool));
        let TypeRef::Map { key, value } = ty else {
            panic!("expected map type");
        };

        assert!(matches!(*key, TypeRef::String));
        assert!(matches!(*value, TypeRef::Primitive(Primitive::Bool)));
    }
}
