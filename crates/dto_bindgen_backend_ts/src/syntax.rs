pub(crate) fn to_snake_case(value: &str) -> String {
    let mut output = String::new();

    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.extend(ch.to_lowercase());
        } else {
            output.push(ch);
        }
    }

    output
}

pub(crate) fn push_object_key(output: &mut String, value: &str) {
    if is_valid_property_identifier(value) {
        output.push_str(value);
    } else {
        output.push('"');
        output.push_str(&escape_string_literal(value));
        output.push('"');
    }
}

pub(crate) fn is_valid_type_identifier(value: &str) -> bool {
    is_valid_identifier(value) && !is_reserved_type_identifier(value)
}

fn is_valid_property_identifier(value: &str) -> bool {
    is_valid_identifier(value) && !is_reserved_type_identifier(value)
}

fn is_valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_identifier_start(first) {
        return false;
    }
    chars.all(is_identifier_part)
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_identifier_part(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}

fn is_reserved_type_identifier(value: &str) -> bool {
    matches!(
        value,
        "abstract"
            | "any"
            | "as"
            | "asserts"
            | "async"
            | "await"
            | "bigint"
            | "boolean"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "constructor"
            | "continue"
            | "debugger"
            | "declare"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "from"
            | "function"
            | "get"
            | "global"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "infer"
            | "instanceof"
            | "interface"
            | "is"
            | "keyof"
            | "let"
            | "module"
            | "namespace"
            | "never"
            | "new"
            | "null"
            | "number"
            | "object"
            | "of"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "require"
            | "return"
            | "set"
            | "static"
            | "string"
            | "super"
            | "switch"
            | "symbol"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "undefined"
            | "unique"
            | "unknown"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
    )
}

pub(crate) fn escape_string_literal(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\u{2028}' => output.push_str("\\u2028"),
            '\u{2029}' => output.push_str("\\u2029"),
            ch if ch.is_control() => {
                use std::fmt::Write as _;
                write!(&mut output, "\\u{:04x}", ch as u32)
                    .expect("writing to a String cannot fail");
            }
            ch => output.push(ch),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_typescript_identifiers() {
        assert!(is_valid_type_identifier("UserProfile"));
        assert!(is_valid_type_identifier("$internal"));
        assert!(!is_valid_type_identifier("user-profile"));
        assert!(!is_valid_type_identifier("123User"));
        assert!(!is_valid_type_identifier("class"));
    }

    #[test]
    fn quotes_unsafe_object_keys() {
        let mut output = String::new();
        push_object_key(&mut output, "content-type");
        assert_eq!(output, "\"content-type\"");
    }

    #[test]
    fn escapes_string_literals() {
        assert_eq!(
            escape_string_literal("admin\"root\\line\nnext\u{2028}sep"),
            "admin\\\"root\\\\line\\nnext\\u2028sep"
        );
    }
}
