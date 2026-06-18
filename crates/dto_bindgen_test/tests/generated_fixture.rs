use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use dto_bindgen::Dto;
use serde::Serialize;
use serde_json::json;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize, Dto)]
#[serde(rename_all = "camelCase")]
struct PostalAddress {
    line_1: String,
}

#[derive(Clone, Serialize, Dto)]
#[serde(rename_all = "camelCase")]
enum UserRole {
    Admin,
    GuestUser,
}

#[derive(Clone, Serialize, Dto)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UserProfile {
    user_id: String,
    active: bool,
    address: PostalAddress,
    tags: Vec<String>,
    role: UserRole,
}

#[derive(Clone, Serialize, Dto)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum SdkEvent {
    UserCreated { user: UserProfile, event_id: String },
}

#[test]
fn serde_fixtures_match_supported_wire_shapes() {
    let profile = user_profile();
    let event = SdkEvent::UserCreated {
        user: profile.clone(),
        event_id: "event-1".to_owned(),
    };

    assert_eq!(
        serde_json::to_value(&profile).unwrap(),
        json!({
            "userId": "user-1",
            "active": true,
            "address": {
                "line1": "1 Main St"
            },
            "tags": ["alpha", "beta"],
            "role": "guestUser"
        })
    );
    assert_eq!(
        serde_json::to_value(UserRole::Admin).unwrap(),
        json!("admin")
    );
    assert_eq!(
        serde_json::to_value(&event).unwrap(),
        json!({
            "type": "userCreated",
            "payload": {
                "user": {
                    "userId": "user-1",
                    "active": true,
                    "address": {
                        "line1": "1 Main St"
                    },
                    "tags": ["alpha", "beta"],
                    "role": "guestUser"
                },
                "eventId": "event-1"
            }
        })
    );
}

#[test]
fn export_is_byte_deterministic_for_generated_fixture() {
    let root = TempProject::new();
    let config_path = write_config(root.path());

    export_fixture(&config_path);
    let first = read_tree(&root.path().join("generated"));
    export_fixture(&config_path);
    let second = read_tree(&root.path().join("generated"));

    assert_eq!(first, second);
    assert!(first.contains_key("dto_bindgen.generated.json"));
    assert!(first.contains_key("ts/user_profile.ts"));
    assert!(first.contains_key("python/my_sdk_dto/user_profile.py"));
}

#[test]
fn generated_python_compiles_imports_and_roundtrips() {
    let root = TempProject::new();
    let config_path = write_config(root.path());
    export_fixture(&config_path);

    let package_dir = root.path().join("generated/python/my_sdk_dto");
    run_python(&[
        "-m",
        "compileall",
        "-q",
        package_dir.to_str().expect("temp path should be UTF-8"),
    ]);

    let python_root = root.path().join("generated/python");
    let python_root_literal =
        serde_json::to_string(python_root.to_str().expect("temp path should be UTF-8")).unwrap();
    let script = format!(
        r#"
import sys

sys.path.insert(0, {python_root_literal})

from my_sdk_dto import PostalAddress, UserProfile, UserRole
from my_sdk_dto.sdk_event import SdkEventUserCreated, parse_sdk_event

profile_data = {{
    "userId": "user-1",
    "active": True,
    "address": {{"line1": "1 Main St"}},
    "tags": ["alpha", "beta"],
    "role": "guestUser",
}}
profile = UserProfile.from_dict(profile_data)
assert isinstance(profile.address, PostalAddress)
assert profile.role == UserRole.GUEST_USER
assert profile.to_dict() == profile_data

event_data = {{
    "type": "userCreated",
    "payload": {{
        "user": profile_data,
        "eventId": "event-1",
    }},
}}
event = parse_sdk_event(event_data)
assert isinstance(event, SdkEventUserCreated)
assert event.to_dict() == event_data

try:
    UserProfile.from_dict({{**profile_data, "extra": True}})
except Exception as exc:
    assert "failed to parse DTO" in str(exc)
else:
    raise AssertionError("deny_unknown_fields should reject extra keys")
"#
    );
    run_python(&["-c", &script]);
}

fn user_profile() -> UserProfile {
    UserProfile {
        user_id: "user-1".to_owned(),
        active: true,
        address: PostalAddress {
            line_1: "1 Main St".to_owned(),
        },
        tags: vec!["alpha".to_owned(), "beta".to_owned()],
        role: UserRole::GuestUser,
    }
}

fn export_fixture(config_path: &Path) -> dto_bindgen::export::ExportReport {
    dto_bindgen::export_types!(
        config = config_path,
        roots = [PostalAddress, UserRole, UserProfile, SdkEvent],
    )
    .unwrap()
}

fn write_config(root: &Path) -> PathBuf {
    let config_path = root.join("dto_bindgen.toml");
    fs::write(
        &config_path,
        r#"
[export]
out_dir = "generated"

[typescript]
enabled = true
out_dir = "generated/ts"

[python]
enabled = true
out_dir = "generated/python/my_sdk_dto"
package = "my_sdk_dto"
"#,
    )
    .unwrap();
    config_path
}

fn read_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    read_tree_into(root, root, &mut files);
    files
}

fn read_tree_into(base: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    let entries = fs::read_dir(dir).unwrap();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            read_tree_into(base, &path, files);
        } else {
            let relative = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, fs::read(path).unwrap());
        }
    }
}

fn run_python(args: &[&str]) {
    let python = std::env::var("DTO_BINDGEN_PYTHON").unwrap_or_else(|_| "python3".to_owned());
    let output = Command::new(&python)
        .args(args)
        .output()
        .unwrap_or_else(|source| panic!("failed to run {python}: {source}"));

    assert!(
        output.status.success(),
        "{python} {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "dto_bindgen_generated_fixture_{}_{}",
            std::process::id(),
            counter
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        self.0.as_path()
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
