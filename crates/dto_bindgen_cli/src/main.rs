#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

const DEFAULT_CONFIG_PATH: &str = "dto_bindgen.toml";
const DEFAULT_INVENTORY_MANIFEST_PATH: &str = "dto_bindgen.inventory.toml";
const DEFAULT_INVENTORY_JSON_PATH: &str = "docs/implementation/reports/sdk_inventory_pilot.json";
const DEFAULT_INVENTORY_MARKDOWN_PATH: &str = "docs/implementation/reports/sdk_inventory_pilot.md";

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut stdout = String::new();
    let mut stderr = String::new();
    let code = run(args, &mut stdout, &mut stderr);

    print!("{stdout}");
    eprint!("{stderr}");
    std::process::exit(code);
}

fn run(args: Vec<String>, stdout: &mut String, stderr: &mut String) -> i32 {
    match parse_args(&args) {
        Ok(CliOptions {
            command: Command::Help,
            ..
        }) => {
            stdout.push_str(help_text());
            0
        }
        Ok(CliOptions {
            command: Command::Version,
            ..
        }) => {
            writeln!(stdout, "dto_bindgen {}", dto_bindgen::version())
                .expect("writing to a String cannot fail");
            0
        }
        Ok(options) => run_command(options, stdout, stderr),
        Err(message) => {
            writeln!(stderr, "error: {message}").expect("writing to a String cannot fail");
            writeln!(stderr).expect("writing to a String cannot fail");
            stderr.push_str(help_text());
            2
        }
    }
}

fn run_command(options: CliOptions, stdout: &mut String, stderr: &mut String) -> i32 {
    match options.command {
        Command::Config => {
            match dto_bindgen::config::Config::from_toml_path(&options.config_path) {
                Ok(config) => {
                    writeln!(stdout, "dto_bindgen config ok")
                        .expect("writing to a String cannot fail");
                    writeln!(stdout, "config = {}", options.config_path.display())
                        .expect("writing to a String cannot fail");
                    writeln!(stdout, "export.out_dir = {}", config.export.out_dir)
                        .expect("writing to a String cannot fail");
                    writeln!(stdout, "typescript.enabled = {}", config.typescript.enabled)
                        .expect("writing to a String cannot fail");
                    writeln!(stdout, "typescript.out_dir = {}", config.typescript.out_dir)
                        .expect("writing to a String cannot fail");
                    writeln!(stdout, "python.enabled = {}", config.python.enabled)
                        .expect("writing to a String cannot fail");
                    writeln!(stdout, "python.out_dir = {}", config.python.out_dir)
                        .expect("writing to a String cannot fail");
                    0
                }
                Err(source) => {
                    writeln!(stderr, "error: {source}").expect("writing to a String cannot fail");
                    1
                }
            }
        }
        Command::Clean => match dto_bindgen::config::Config::from_toml_path(&options.config_path) {
            Ok(config) => match dto_bindgen_core::OutputWriter::clean_previous_manifest_at(
                output_root(&options.config_path, &config.export.out_dir),
            ) {
                Ok(report) => {
                    writeln!(
                        stdout,
                        "removed {} manifest-owned file(s)",
                        report.files.len()
                    )
                    .expect("writing to a String cannot fail");
                    0
                }
                Err(source) => {
                    writeln!(stderr, "error: {source}").expect("writing to a String cannot fail");
                    1
                }
            },
            Err(source) => {
                writeln!(stderr, "error: {source}").expect("writing to a String cannot fail");
                1
            }
        },
        Command::Inventory => run_inventory(&options, stdout, stderr),
        Command::Export | Command::Check | Command::Diagnostics => {
            match dto_bindgen::config::Config::from_toml_path(&options.config_path) {
                Ok(_) => report_explicit_roots_required(&options, stderr),
                Err(source) => {
                    writeln!(stderr, "error: {source}").expect("writing to a String cannot fail");
                    1
                }
            }
        }
        Command::Help | Command::Version => unreachable!("handled before run_command"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    command: Command,
    config_path: PathBuf,
    inventory_manifest_path: PathBuf,
    inventory_json_path: Option<PathBuf>,
    inventory_markdown_path: Option<PathBuf>,
    json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    Config,
    Export,
    Check,
    Clean,
    Inventory,
    Diagnostics,
}

impl Command {
    fn as_str(self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Version => "version",
            Self::Config => "config",
            Self::Export => "export",
            Self::Check => "check",
            Self::Clean => "clean",
            Self::Inventory => "inventory",
            Self::Diagnostics => "diagnostics",
        }
    }
}

fn parse_args(args: &[String]) -> Result<CliOptions, String> {
    if args.is_empty() {
        return Ok(default_options(Command::Help));
    }

    let command = match args[0].as_str() {
        "-h" | "--help" => return Ok(default_options(Command::Help)),
        "-V" | "--version" => return Ok(default_options(Command::Version)),
        "config" => Command::Config,
        "export" => Command::Export,
        "check" => Command::Check,
        "clean" => Command::Clean,
        "inventory" => Command::Inventory,
        "diagnostics" => Command::Diagnostics,
        value => return Err(format!("unknown command or option `{value}`")),
    };

    let mut options = default_options(command);
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => return Ok(default_options(Command::Help)),
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--config" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--config requires a path".to_owned());
                };
                options.config_path = PathBuf::from(value);
                index += 2;
            }
            "--manifest" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--manifest requires a path".to_owned());
                };
                options.inventory_manifest_path = PathBuf::from(value);
                index += 2;
            }
            "--json-out" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--json-out requires a path".to_owned());
                };
                options.inventory_json_path = Some(PathBuf::from(value));
                index += 2;
            }
            "--markdown-out" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--markdown-out requires a path".to_owned());
                };
                options.inventory_markdown_path = Some(PathBuf::from(value));
                index += 2;
            }
            value if value.starts_with("--config=") => {
                let value = value.trim_start_matches("--config=");
                if value.is_empty() {
                    return Err("--config requires a path".to_owned());
                }
                options.config_path = PathBuf::from(value);
                index += 1;
            }
            value if value.starts_with("--manifest=") => {
                let value = value.trim_start_matches("--manifest=");
                if value.is_empty() {
                    return Err("--manifest requires a path".to_owned());
                }
                options.inventory_manifest_path = PathBuf::from(value);
                index += 1;
            }
            value if value.starts_with("--json-out=") => {
                let value = value.trim_start_matches("--json-out=");
                if value.is_empty() {
                    return Err("--json-out requires a path".to_owned());
                }
                options.inventory_json_path = Some(PathBuf::from(value));
                index += 1;
            }
            value if value.starts_with("--markdown-out=") => {
                let value = value.trim_start_matches("--markdown-out=");
                if value.is_empty() {
                    return Err("--markdown-out requires a path".to_owned());
                }
                options.inventory_markdown_path = Some(PathBuf::from(value));
                index += 1;
            }
            value => return Err(format!("unknown option `{value}`")),
        }
    }

    Ok(options)
}

fn default_options(command: Command) -> CliOptions {
    CliOptions {
        command,
        config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
        inventory_manifest_path: PathBuf::from(DEFAULT_INVENTORY_MANIFEST_PATH),
        inventory_json_path: None,
        inventory_markdown_path: None,
        json: false,
    }
}

fn help_text() -> &'static str {
    concat!(
        "dto_bindgen: generate DTO bindings from explicit Rust roots\n\n",
        "Usage:\n",
        "  dto_bindgen --help\n",
        "  dto_bindgen --version\n",
        "  dto_bindgen config [--config <path>]\n",
        "  dto_bindgen export [--config <path>]\n",
        "  dto_bindgen check [--config <path>]\n",
        "  dto_bindgen clean [--config <path>]\n",
        "  dto_bindgen inventory [--manifest <path>] [--json-out <path>] [--markdown-out <path>]\n",
        "  dto_bindgen diagnostics [--config <path>] [--json]\n\n",
        "Commands:\n",
        "  config       Load and summarize dto_bindgen.toml without exporting.\n",
        "  export       Requires an explicit-root Rust harness in the MVP.\n",
        "  check        Requires an explicit-root Rust harness in the MVP.\n",
        "  clean        Remove files listed in the previous generated manifest.\n",
        "  inventory    Scan explicit SDK source inputs and write JSON/Markdown reports.\n",
        "  diagnostics  Reserved for structured diagnostic output.\n\n",
        "Inventory reports are explicit-input only; the CLI does not magically discover every Rust root.\n\n",
        "Explicit root export example:\n",
        "  dto_bindgen::export_types!(config = \"dto_bindgen.toml\", roots = [UserProfile, SdkEvent])\n",
    )
}

fn run_inventory(options: &CliOptions, stdout: &mut String, stderr: &mut String) -> i32 {
    match run_inventory_inner(options) {
        Ok(report_paths) => {
            writeln!(stdout, "dto_bindgen inventory ok").expect("writing to a String cannot fail");
            writeln!(stdout, "json = {}", report_paths.json_path.display())
                .expect("writing to a String cannot fail");
            writeln!(
                stdout,
                "markdown = {}",
                report_paths.markdown_path.display()
            )
            .expect("writing to a String cannot fail");
            0
        }
        Err(message) => {
            writeln!(stderr, "error: {message}").expect("writing to a String cannot fail");
            1
        }
    }
}

struct InventoryReportPaths {
    json_path: PathBuf,
    markdown_path: PathBuf,
}

fn run_inventory_inner(options: &CliOptions) -> Result<InventoryReportPaths, String> {
    let manifest =
        dto_bindgen_core::InventoryManifest::from_toml_path(&options.inventory_manifest_path)
            .map_err(|source| source.to_string())?;

    if manifest.sdk.source_files.is_empty() {
        return Err("inventory manifest must list at least one sdk.source_files entry".to_owned());
    }

    let manifest_dir = options
        .inventory_manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let sdk_root = resolve_path(manifest_dir, &manifest.sdk.root);
    let mut inventories = Vec::new();

    for source_file in &manifest.sdk.source_files {
        let source_path = resolve_path(&sdk_root, source_file);
        let input = fs::read_to_string(&source_path)
            .map_err(|source| format!("failed to read `{}`: {source}", source_path.display()))?;
        inventories.push(
            dto_bindgen_core::scan_rust_source(source_file.clone(), &input)
                .map_err(|source| source.to_string())?,
        );
    }

    let report = dto_bindgen_core::build_inventory_report(manifest, inventories);
    let json = dto_bindgen_core::render_inventory_json(&report)
        .map_err(|source| format!("failed to render inventory JSON: {source}"))?;
    let markdown = dto_bindgen_core::render_inventory_markdown(&report);
    let json_path = options
        .inventory_json_path
        .clone()
        .unwrap_or_else(|| manifest_dir.join(DEFAULT_INVENTORY_JSON_PATH));
    let markdown_path = options
        .inventory_markdown_path
        .clone()
        .unwrap_or_else(|| manifest_dir.join(DEFAULT_INVENTORY_MARKDOWN_PATH));

    write_report_file(&json_path, &json)?;
    write_report_file(&markdown_path, &markdown)?;

    Ok(InventoryReportPaths {
        json_path,
        markdown_path,
    })
}

fn resolve_path(base: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn write_report_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| format!("failed to create `{}`: {source}", parent.display()))?;
    }
    fs::write(path, contents)
        .map_err(|source| format!("failed to write `{}`: {source}", path.display()))
}

fn report_explicit_roots_required(options: &CliOptions, stderr: &mut String) -> i32 {
    if options.json {
        writeln!(
            stderr,
            "{{\"error\":\"explicit_roots_required\",\"command\":\"{}\",\"config\":\"{}\"}}",
            options.command.as_str(),
            escape_json(&options.config_path.display().to_string())
        )
        .expect("writing to a String cannot fail");
    } else {
        writeln!(
            stderr,
            "error: dto_bindgen {} requires explicit root descriptors",
            options.command.as_str()
        )
        .expect("writing to a String cannot fail");
        writeln!(
            stderr,
            "hint: call dto_bindgen::export_types!(config = \"{}\", roots = [...]) from a test, xtask, or export binary",
            options.config_path.display()
        )
        .expect("writing to a String cannot fail");
        writeln!(
            stderr,
            "hint: the standalone CLI does not discover Rust roots in the MVP"
        )
        .expect("writing to a String cannot fail");
    }
    2
}

fn output_root(config_path: &std::path::Path, out_dir: &str) -> PathBuf {
    let base = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    base.join(out_dir)
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn help_is_default_command() {
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(Vec::new(), &mut stdout, &mut stderr);

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("Usage:"));
        assert!(stdout.contains("explicit-root Rust harness"));
    }

    #[test]
    fn parses_config_equals_path() {
        let options = parse_args(&[
            "config".to_owned(),
            "--config=custom/dto_bindgen.toml".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.command, Command::Config);
        assert_eq!(
            options.config_path,
            PathBuf::from("custom/dto_bindgen.toml")
        );
    }

    #[test]
    fn parses_inventory_report_paths() {
        let options = parse_args(&[
            "inventory".to_owned(),
            "--manifest=inventory.toml".to_owned(),
            "--json-out".to_owned(),
            "report.json".to_owned(),
            "--markdown-out=report.md".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.command, Command::Inventory);
        assert_eq!(
            options.inventory_manifest_path,
            PathBuf::from("inventory.toml")
        );
        assert_eq!(
            options.inventory_json_path,
            Some(PathBuf::from("report.json"))
        );
        assert_eq!(
            options.inventory_markdown_path,
            Some(PathBuf::from("report.md"))
        );
    }

    #[test]
    fn config_command_loads_and_summarizes_config() {
        let root = temp_project();
        let config_path = root.join("dto_bindgen.toml");
        std::fs::write(
            &config_path,
            "[export]\nout_dir = \"generated\"\n[python]\nenabled = false\n",
        )
        .unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "config".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("dto_bindgen config ok"));
        assert!(stdout.contains("python.enabled = false"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn check_command_requires_explicit_roots() {
        let root = temp_project();
        let config_path = root.join("dto_bindgen.toml");
        std::fs::write(&config_path, "").unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "check".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(stderr.contains("requires explicit root descriptors"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clean_command_removes_manifest_owned_files() {
        let root = temp_project();
        let config_path = root.join("dto_bindgen.toml");
        let generated = root.join("generated");
        let generated_ts = generated.join("ts");
        std::fs::create_dir_all(&generated_ts).unwrap();
        std::fs::write(&config_path, "[export]\nout_dir = \"generated\"\n").unwrap();
        std::fs::write(generated_ts.join("old.ts"), "old\n").unwrap();
        std::fs::write(generated_ts.join("keep.ts"), "keep\n").unwrap();
        std::fs::write(
            generated.join("dto_bindgen.generated.json"),
            r#"{
  "generator": "dto_bindgen",
  "version": "0.1.0",
  "registry_hash": "registry",
  "config_hash": "config",
  "files": [
    {
      "backend": "typescript",
      "path": "ts/old.ts",
      "sha256": "digest"
    }
  ]
}
"#,
        )
        .unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "clean".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("removed 1 manifest-owned file"));
        assert!(!generated_ts.join("old.ts").exists());
        assert!(!generated.join("dto_bindgen.generated.json").exists());
        assert_eq!(
            std::fs::read_to_string(generated_ts.join("keep.ts")).unwrap(),
            "keep\n"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clean_command_noops_when_output_root_is_absent() {
        let root = temp_project();
        let config_path = root.join("dto_bindgen.toml");
        let generated = root.join("generated");
        std::fs::write(&config_path, "[export]\nout_dir = \"generated\"\n").unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "clean".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("removed 0 manifest-owned file"));
        assert!(!generated.exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inventory_command_writes_reports_without_generated_output() {
        let root = temp_project();
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("sdk.rs"),
            r#"
#[derive(Serialize, Dto)]
struct UserProfile {
    #[serde(skip)]
    internal_note: String,
    id: uuid::Uuid,
}
"#,
        )
        .unwrap();
        let manifest_path = root.join("dto_bindgen.inventory.toml");
        let json_path = root.join("reports/inventory.json");
        let markdown_path = root.join("reports/inventory.md");
        std::fs::write(
            &manifest_path,
            r#"
roots = ["UserProfile"]

[sdk]
root = "."
package = "sdk"
source_files = ["src/sdk.rs"]

[typescript]
generated_artifact_policy = "checked_in"

[typescript.package_shape]
out_dir = "generated/ts"
package = "sdk"

[python]
generated_artifact_policy = "build_time"

[python.package_shape]
out_dir = "generated/python/sdk_dto"
package = "sdk_dto"
"#,
        )
        .unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "inventory".to_owned(),
                "--manifest".to_owned(),
                manifest_path.display().to_string(),
                "--json-out".to_owned(),
                json_path.display().to_string(),
                "--markdown-out".to_owned(),
                markdown_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("dto_bindgen inventory ok"));
        assert!(
            std::fs::read_to_string(&json_path)
                .unwrap()
                .contains("\"schema_version\": 1")
        );
        assert!(
            std::fs::read_to_string(&markdown_path)
                .unwrap()
                .contains("## Promotion Decisions")
        );
        assert!(!root.join("generated").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diagnostics_command_can_report_json_root_requirement() {
        let root = temp_project();
        let config_path = root.join("dto_bindgen.toml");
        std::fs::write(&config_path, "").unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "diagnostics".to_owned(),
                "--json".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(stderr.contains("\"explicit_roots_required\""));

        std::fs::remove_dir_all(root).unwrap();
    }

    fn temp_project() -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "dto_bindgen_cli_test_{}_{}",
            std::process::id(),
            counter
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        root
    }
}
