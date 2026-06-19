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
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    preferences: BTreeMap<String, String>,
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

#[derive(Clone, Serialize, Dto)]
#[serde(rename_all = "camelCase")]
struct EventEnvelope {
    source: String,
    event: SdkEvent,
}

#[derive(Clone, Serialize, Dto)]
#[serde(rename_all = "camelCase")]
struct PackEntry {
    kind: String,
    url: String,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
}

#[derive(Clone, Serialize, Dto)]
#[serde(rename_all = "camelCase")]
struct RuntimeWebPackAsset {
    locale: String,
    url: String,
    kind: String,
    hash: String,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
}

#[derive(Clone, Serialize, Dto)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema: u32,
    mf2_packs: BTreeMap<String, PackEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    budgets: Option<BTreeMap<String, u64>>,
}

#[test]
fn serde_fixtures_match_supported_wire_shapes() {
    let profile = user_profile();
    let event = SdkEvent::UserCreated {
        user: profile.clone(),
        event_id: "event-1".to_owned(),
    };
    let envelope = EventEnvelope {
        source: "fixture".to_owned(),
        event,
    };
    let manifest = manifest_fixture();

    assert_eq!(
        serde_json::to_value(&profile).unwrap(),
        json!({
            "userId": "user-1",
            "active": true,
            "address": {
                "line1": "1 Main St"
            },
            "tags": ["alpha", "beta"],
            "role": "guestUser",
            "aliases": [],
            "preferences": {}
        })
    );
    assert_eq!(
        serde_json::to_value(UserRole::Admin).unwrap(),
        json!("admin")
    );
    assert_eq!(
        serde_json::to_value(&envelope.event).unwrap(),
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
                    "role": "guestUser",
                    "aliases": [],
                    "preferences": {}
                },
                "eventId": "event-1"
            }
        })
    );
    assert_eq!(
        serde_json::to_value(&envelope).unwrap(),
        json!({
            "source": "fixture",
            "event": {
                "type": "userCreated",
                "payload": {
                    "user": {
                        "userId": "user-1",
                        "active": true,
                        "address": {
                            "line1": "1 Main St"
                        },
                        "tags": ["alpha", "beta"],
                        "role": "guestUser",
                        "aliases": [],
                        "preferences": {}
                    },
                    "eventId": "event-1"
                }
            }
        })
    );
    assert_eq!(
        serde_json::to_value(&manifest).unwrap(),
        json!({
            "schema": 1,
            "mf2Packs": {
                "en": {
                    "kind": "base",
                    "url": "packs/en.mf2pack",
                    "size": 42
                }
            },
            "budgets": {
                "initial": 4096
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
    assert!(first.contains_key("ts/manifest.ts"));
    assert!(first.contains_key("python/my_sdk_dto/user_profile.py"));
    assert!(first.contains_key("python/my_sdk_dto/manifest.py"));
    let user_ts = String::from_utf8(first["ts/user_profile.ts"].clone()).unwrap();
    assert!(user_ts.contains("displayName?: string | null;"));
    assert!(user_ts.contains("aliases?: Array<string>;"));
    assert!(user_ts.contains("preferences?: Record<string, string>;"));
    let pack_ts = String::from_utf8(first["ts/pack_entry.ts"].clone()).unwrap();
    assert!(pack_ts.contains("size: number;"));
    assert!(pack_ts.contains("parent?: string | null;"));
    let manifest_ts = String::from_utf8(first["ts/manifest.ts"].clone()).unwrap();
    assert!(manifest_ts.contains("budgets?: Record<string, number> | null;"));
    let user_py = String::from_utf8(first["python/my_sdk_dto/user_profile.py"].clone()).unwrap();
    assert!(user_py.contains("display_name: str | None = field(default=None"));
    assert!(user_py.contains("aliases: list[str] = field(default_factory=list"));
    assert!(user_py.contains("preferences: dict[str, str] = field(default_factory=dict"));
    assert!(user_py.contains("data.get(\"displayName\")"));
    assert!(user_py.contains("if self.display_name is not None"));
    assert!(user_py.contains("data.get(\"aliases\", [])"));
    assert!(user_py.contains("data.get(\"preferences\", {})"));
    let envelope_py =
        String::from_utf8(first["python/my_sdk_dto/event_envelope.py"].clone()).unwrap();
    assert!(envelope_py.contains("from .sdk_event import SdkEvent, parse_sdk_event"));
    assert!(envelope_py.contains("parse_sdk_event(data[\"event\"])"));
    let manifest_py = String::from_utf8(first["python/my_sdk_dto/manifest.py"].clone()).unwrap();
    assert!(manifest_py.contains("budgets: dict[str, int] | None"));
    assert!(manifest_py.contains("data.get(\"budgets\")"));
    assert!(manifest_py.contains("if self.budgets is not None"));
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

from my_sdk_dto import EventEnvelope, Manifest, PackEntry, PostalAddress, UserProfile, UserRole
from my_sdk_dto.sdk_event import SdkEventUserCreated, parse_sdk_event

profile_data = {{
    "userId": "user-1",
    "active": True,
    "address": {{"line1": "1 Main St"}},
    "tags": ["alpha", "beta"],
    "role": "guestUser",
    "displayName": None,
    "aliases": [],
    "preferences": {{}},
}}
profile_output = {{
    key: value for key, value in profile_data.items() if key != "displayName"
}}
profile = UserProfile.from_dict(profile_data)
assert isinstance(profile.address, PostalAddress)
assert profile.role == UserRole.GUEST_USER
assert profile.to_dict() == profile_output

profile_missing_defaults = {{
    "userId": "user-1",
    "active": True,
    "address": {{"line1": "1 Main St"}},
    "tags": ["alpha", "beta"],
    "role": "guestUser",
}}
profile_with_defaults = UserProfile.from_dict(profile_missing_defaults)
assert profile_with_defaults.display_name is None
assert profile_with_defaults.aliases == []
assert profile_with_defaults.preferences == {{}}
assert profile_with_defaults.to_dict() == {{
    **profile_missing_defaults,
    "aliases": [],
    "preferences": {{}},
}}

event_data = {{
    "type": "userCreated",
    "payload": {{
        "user": profile_output,
        "eventId": "event-1",
    }},
}}
event = parse_sdk_event(event_data)
assert isinstance(event, SdkEventUserCreated)
assert event.to_dict() == event_data

envelope_data = {{
    "source": "fixture",
    "event": event_data,
}}
envelope = EventEnvelope.from_dict(envelope_data)
assert isinstance(envelope.event, SdkEventUserCreated)
assert envelope.to_dict() == envelope_data

manifest_data = {{
    "schema": 1,
    "mf2Packs": {{
        "en": {{
            "kind": "base",
            "url": "packs/en.mf2pack",
            "size": 42,
        }},
    }},
    "budgets": {{"initial": 4096}},
}}
manifest = Manifest.from_dict(manifest_data)
assert isinstance(manifest.mf2_packs["en"], PackEntry)
assert manifest.mf2_packs["en"].size == 42
assert manifest.budgets == {{"initial": 4096}}
assert manifest.to_dict() == manifest_data

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

#[test]
fn generated_typescript_typechecks_strict() {
    if std::env::var_os("DTO_BINDGEN_RUN_TSC").is_none() {
        eprintln!("skipping strict TypeScript check; set DTO_BINDGEN_RUN_TSC=1 to enable");
        return;
    }

    let root = TempProject::new();
    let config_path = write_config(root.path());
    export_fixture(&config_path);
    let tsconfig_path = write_tsconfig(root.path());

    run_tsc(&[
        "--noEmit",
        "--strict",
        "--project",
        tsconfig_path.to_str().expect("temp path should be UTF-8"),
    ]);
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
        display_name: None,
        aliases: Vec::new(),
        preferences: BTreeMap::new(),
    }
}

fn manifest_fixture() -> Manifest {
    let mut mf2_packs = BTreeMap::new();
    mf2_packs.insert(
        "en".to_owned(),
        PackEntry {
            kind: "base".to_owned(),
            url: "packs/en.mf2pack".to_owned(),
            size: 42,
            parent: None,
        },
    );
    let mut budgets = BTreeMap::new();
    budgets.insert("initial".to_owned(), 4096);

    Manifest {
        schema: 1,
        mf2_packs,
        budgets: Some(budgets),
    }
}

fn export_fixture(config_path: &Path) -> dto_bindgen::export::ExportReport {
    dto_bindgen::export_types!(
        config = config_path,
        roots = [
            PostalAddress,
            UserRole,
            UserProfile,
            SdkEvent,
            EventEnvelope,
            PackEntry,
            RuntimeWebPackAsset,
            Manifest
        ],
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

[numeric]
large_int_policy = "json_number_unsafe"

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

fn write_tsconfig(root: &Path) -> PathBuf {
    let tsconfig_path = root.join("tsconfig.generated.json");
    fs::write(
        &tsconfig_path,
        r#"
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true
  },
  "include": ["generated/ts/**/*.ts"]
}
"#,
    )
    .unwrap();
    tsconfig_path
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

fn run_tsc(args: &[&str]) {
    let configured = std::env::var("DTO_BINDGEN_TSC").ok();
    let mut command = match configured {
        Some(tsc) => Command::new(tsc),
        None => {
            let mut command = Command::new("npx");
            command.arg("tsc");
            command
        }
    };
    let output = command
        .args(args)
        .output()
        .unwrap_or_else(|source| panic!("failed to run TypeScript compiler: {source}"));

    assert!(
        output.status.success(),
        "tsc {:?} failed\nstdout:\n{}\nstderr:\n{}",
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
