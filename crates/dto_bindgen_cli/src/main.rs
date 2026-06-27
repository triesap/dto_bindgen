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
                    writeln!(
                        stdout,
                        "root_discovery.mode = {}",
                        root_discovery_mode_name(config.root_discovery.mode)
                    )
                    .expect("writing to a String cannot fail");
                    writeln!(
                        stdout,
                        "root_discovery.source_files = {}",
                        config.root_discovery.source_files.len()
                    )
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
        Command::Export | Command::Check => {
            match dto_bindgen::config::Config::from_toml_path(&options.config_path) {
                Ok(_) => report_explicit_roots_required(&options, stderr),
                Err(source) => {
                    writeln!(stderr, "error: {source}").expect("writing to a String cannot fail");
                    1
                }
            }
        }
        Command::Roots | Command::RootsCheck | Command::Diagnostics => {
            match dto_bindgen::config::Config::from_toml_path(&options.config_path) {
                Ok(config) if has_source_manifest_roots(&config) => {
                    run_source_manifest_command(&options, &config, stdout, stderr)
                }
                Ok(_) if options.command == Command::Diagnostics => {
                    report_explicit_roots_required(&options, stderr)
                }
                Ok(_) => report_source_manifest_required(&options, stderr),
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
    Roots,
    RootsCheck,
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
            Self::Roots => "roots",
            Self::RootsCheck => "roots-check",
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
        "roots" => Command::Roots,
        "roots-check" => Command::RootsCheck,
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
        "dto_bindgen: generate DTO bindings from explicit Rust roots or source manifests\n\n",
        "Usage:\n",
        "  dto_bindgen --help\n",
        "  dto_bindgen --version\n",
        "  dto_bindgen config [--config <path>]\n",
        "  dto_bindgen export [--config <path>]\n",
        "  dto_bindgen check [--config <path>]\n",
        "  dto_bindgen roots [--config <path>]\n",
        "  dto_bindgen roots-check [--config <path>]\n",
        "  dto_bindgen clean [--config <path>]\n",
        "  dto_bindgen inventory [--manifest <path>] [--json-out <path>] [--markdown-out <path>]\n",
        "  dto_bindgen diagnostics [--config <path>] [--json]\n\n",
        "Commands:\n",
        "  config       Load and summarize dto_bindgen.toml without exporting.\n",
        "  export       Requires compiled explicit root descriptors and writes backend output.\n",
        "  check        Requires compiled explicit root descriptors and checks backend output.\n",
        "  roots        Write generated Rust root modules for source_manifest configs.\n",
        "  roots-check  Check generated Rust root modules for source_manifest configs.\n",
        "  clean        Remove files listed in the previous generated manifest.\n",
        "  inventory    Scan explicit SDK source inputs and write JSON/Markdown reports.\n",
        "  diagnostics  Report explicit-root or source-manifest diagnostics.\n\n",
        "Source manifests are explicit-input only; the CLI scans only configured source files.\n\n",
        "Source manifest example:\n",
        "  [root_discovery]\n",
        "  mode = \"source_manifest\"\n",
        "  source_files = [\"src/lib.rs\"]\n",
        "  root_module_file = \"src/generated/dto_roots.rs\"\n\n",
        "Generated roots:\n",
        "  dto_bindgen roots --config dto_bindgen.toml\n",
        "  dto_bindgen roots-check --config dto_bindgen.toml\n\n",
        "Compiled export example:\n",
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

fn run_source_manifest_command(
    options: &CliOptions,
    config: &dto_bindgen::config::Config,
    stdout: &mut String,
    stderr: &mut String,
) -> i32 {
    match build_source_manifest_roots(options, config) {
        Ok(root_modules) => match options.command {
            Command::Roots => write_source_manifest_roots(&root_modules, stdout, stderr),
            Command::RootsCheck => check_source_manifest_roots(&root_modules, stdout, stderr),
            Command::Diagnostics => {
                report_source_manifest_diagnostics(options, &root_modules, stdout);
                0
            }
            Command::Help
            | Command::Version
            | Command::Config
            | Command::Export
            | Command::Check
            | Command::Clean
            | Command::Inventory => {
                unreachable!("source manifest routing is limited to export/check/diagnostics")
            }
        },
        Err(message) => {
            writeln!(stderr, "error: {message}").expect("writing to a String cannot fail");
            1
        }
    }
}

#[derive(Debug, Clone)]
struct SourceManifestRoots {
    package_key: Option<String>,
    root_module_path: PathBuf,
    source_files: Vec<String>,
    module: dto_bindgen_core::GeneratedRootModule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RootModuleStatus {
    Missing,
    Current,
    Stale,
    ReadError(String),
}

impl RootModuleStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Current => "current",
            Self::Stale => "stale",
            Self::ReadError(_) => "read_error",
        }
    }
}

fn build_source_manifest_roots(
    options: &CliOptions,
    config: &dto_bindgen::config::Config,
) -> Result<Vec<SourceManifestRoots>, String> {
    source_manifest_jobs(config)
        .into_iter()
        .map(|job| build_source_manifest_root(options, config, job))
        .collect()
}

struct SourceManifestJob {
    package_key: Option<String>,
    source_files: Vec<String>,
    root_module_file: String,
}

fn source_manifest_jobs(config: &dto_bindgen::config::Config) -> Vec<SourceManifestJob> {
    if config.root_discovery.mode == dto_bindgen::config::RootDiscoveryMode::SourceManifest {
        return vec![SourceManifestJob {
            package_key: None,
            source_files: config.root_discovery.source_files.clone(),
            root_module_file: config.root_discovery.root_module_file.clone(),
        }];
    }

    config
        .packages
        .iter()
        .filter(|package| {
            package.root_discovery.mode == dto_bindgen::config::RootDiscoveryMode::SourceManifest
        })
        .map(|package| SourceManifestJob {
            package_key: Some(package.key.clone()),
            source_files: package.root_discovery.source_files.clone(),
            root_module_file: package.root_discovery.root_module_file.clone(),
        })
        .collect()
}

fn build_source_manifest_root(
    options: &CliOptions,
    config: &dto_bindgen::config::Config,
    job: SourceManifestJob,
) -> Result<SourceManifestRoots, String> {
    let config_dir = config_dir(&options.config_path);
    let mut inventories = Vec::new();

    for source_file in &job.source_files {
        let source_path = resolve_path(config_dir, source_file);
        let input = fs::read_to_string(&source_path)
            .map_err(|source| format!("failed to read `{}`: {source}", source_path.display()))?;
        inventories.push(
            dto_bindgen_core::scan_rust_source(source_file.clone(), &input)
                .map_err(|source| source.to_string())?,
        );
    }

    let mut root_config = config.clone();
    root_config.root_discovery.mode = dto_bindgen::config::RootDiscoveryMode::SourceManifest;
    root_config.root_discovery.source_files = job.source_files.clone();
    root_config.root_discovery.root_module_file = job.root_module_file;

    let module = dto_bindgen_core::generate_root_module(&root_config, &inventories)
        .map_err(|source| format!("failed to generate dto root module: {source}"))?;
    let root_module_path = resolve_path(config_dir, &module.path);

    Ok(SourceManifestRoots {
        package_key: job.package_key,
        root_module_path,
        source_files: root_config.root_discovery.source_files,
        module,
    })
}

fn write_source_manifest_roots(
    root_modules: &[SourceManifestRoots],
    stdout: &mut String,
    stderr: &mut String,
) -> i32 {
    for roots in root_modules {
        if let Err(message) = write_report_file(&roots.root_module_path, &roots.module.contents) {
            writeln!(stderr, "error: {message}").expect("writing to a String cannot fail");
            return 1;
        }
    }

    writeln!(stdout, "dto_bindgen roots ok").expect("writing to a String cannot fail");
    write_source_manifest_summary(stdout, root_modules);
    0
}

fn check_source_manifest_roots(
    root_modules: &[SourceManifestRoots],
    stdout: &mut String,
    stderr: &mut String,
) -> i32 {
    for roots in root_modules {
        match inspect_root_module_status(roots) {
            RootModuleStatus::Current => {}
            RootModuleStatus::Stale => {
                writeln!(
                    stderr,
                    "error: generated root module is stale: {}",
                    roots.root_module_path.display()
                )
                .expect("writing to a String cannot fail");
                return 1;
            }
            RootModuleStatus::Missing => {
                writeln!(
                    stderr,
                    "error: generated root module is missing: {}",
                    roots.root_module_path.display()
                )
                .expect("writing to a String cannot fail");
                return 1;
            }
            RootModuleStatus::ReadError(message) => {
                writeln!(
                    stderr,
                    "error: failed to read generated root module `{}`: {message}",
                    roots.root_module_path.display()
                )
                .expect("writing to a String cannot fail");
                return 1;
            }
        }
    }

    writeln!(stdout, "dto_bindgen roots ok").expect("writing to a String cannot fail");
    write_source_manifest_summary(stdout, root_modules);
    0
}

fn inspect_root_module_status(roots: &SourceManifestRoots) -> RootModuleStatus {
    match fs::read_to_string(&roots.root_module_path) {
        Ok(existing) if existing == roots.module.contents => RootModuleStatus::Current,
        Ok(_) => RootModuleStatus::Stale,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => RootModuleStatus::Missing,
        Err(source) => RootModuleStatus::ReadError(source.to_string()),
    }
}

fn report_source_manifest_diagnostics(
    options: &CliOptions,
    root_modules: &[SourceManifestRoots],
    stdout: &mut String,
) {
    if options.json {
        write_source_manifest_diagnostics_json(stdout, root_modules);
        return;
    }

    writeln!(stdout, "dto_bindgen diagnostics ok").expect("writing to a String cannot fail");
    writeln!(stdout, "root_discovery.mode = source_manifest")
        .expect("writing to a String cannot fail");
    write_source_manifest_summary(stdout, root_modules);
    for roots in root_modules {
        write_source_manifest_detail(stdout, roots);
    }
}

fn write_source_manifest_summary(stdout: &mut String, root_modules: &[SourceManifestRoots]) {
    writeln!(stdout, "root_modules = {}", root_modules.len())
        .expect("writing to a String cannot fail");
    writeln!(
        stdout,
        "source_files = {}",
        source_manifest_file_count(root_modules)
    )
    .expect("writing to a String cannot fail");
    writeln!(
        stdout,
        "roots = {}",
        source_manifest_root_count(root_modules)
    )
    .expect("writing to a String cannot fail");
}

fn write_source_manifest_detail(stdout: &mut String, roots: &SourceManifestRoots) {
    if let Some(package_key) = &roots.package_key {
        writeln!(stdout, "package = {package_key}").expect("writing to a String cannot fail");
    }
    writeln!(stdout, "root_module = {}", roots.root_module_path.display())
        .expect("writing to a String cannot fail");
    writeln!(
        stdout,
        "status = {}",
        inspect_root_module_status(roots).as_str()
    )
    .expect("writing to a String cannot fail");
    for source_file in &roots.source_files {
        writeln!(stdout, "source_file = {source_file}").expect("writing to a String cannot fail");
    }
    for root in &roots.module.roots {
        writeln!(
            stdout,
            "root = {} {} {}",
            root.rust_name, root.type_path, root.source_file
        )
        .expect("writing to a String cannot fail");
    }
}

fn write_source_manifest_diagnostics_json(
    stdout: &mut String,
    root_modules: &[SourceManifestRoots],
) {
    write!(
        stdout,
        "{{\"root_discovery\":{{\"mode\":\"source_manifest\",\"root_module_count\":{},\"source_file_count\":{},\"root_count\":{},\"root_modules\":[",
        root_modules.len(),
        source_manifest_file_count(root_modules),
        source_manifest_root_count(root_modules)
    )
    .expect("writing to a String cannot fail");

    for (index, roots) in root_modules.iter().enumerate() {
        if index > 0 {
            stdout.push(',');
        }
        write_source_manifest_roots_json(stdout, roots);
    }

    writeln!(stdout, "]}}}}").expect("writing to a String cannot fail");
}

fn write_source_manifest_roots_json(stdout: &mut String, roots: &SourceManifestRoots) {
    let status = inspect_root_module_status(roots);
    stdout.push('{');
    match &roots.package_key {
        Some(package_key) => {
            write!(stdout, "\"package\":\"{}\",", escape_json(package_key))
                .expect("writing to a String cannot fail");
        }
        None => stdout.push_str("\"package\":null,"),
    }
    write!(
        stdout,
        "\"root_module\":\"{}\",\"status\":\"{}\",\"source_files\":",
        escape_json(&roots.root_module_path.display().to_string()),
        status.as_str()
    )
    .expect("writing to a String cannot fail");
    write_json_string_array(stdout, &roots.source_files);
    write!(
        stdout,
        ",\"source_file_count\":{},\"root_count\":{},\"roots\":[",
        roots.source_files.len(),
        roots.module.roots.len()
    )
    .expect("writing to a String cannot fail");
    for (index, root) in roots.module.roots.iter().enumerate() {
        if index > 0 {
            stdout.push(',');
        }
        write!(
            stdout,
            "{{\"rust_name\":\"{}\",\"type_path\":\"{}\",\"source_file\":\"{}\"}}",
            escape_json(&root.rust_name),
            escape_json(&root.type_path),
            escape_json(&root.source_file)
        )
        .expect("writing to a String cannot fail");
    }
    stdout.push(']');
    if let RootModuleStatus::ReadError(message) = status {
        write!(stdout, ",\"status_error\":\"{}\"", escape_json(&message))
            .expect("writing to a String cannot fail");
    }
    stdout.push('}');
}

fn write_json_string_array(stdout: &mut String, values: &[String]) {
    stdout.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            stdout.push(',');
        }
        write!(stdout, "\"{}\"", escape_json(value)).expect("writing to a String cannot fail");
    }
    stdout.push(']');
}

fn source_manifest_file_count(root_modules: &[SourceManifestRoots]) -> usize {
    root_modules
        .iter()
        .map(|roots| roots.source_files.len())
        .sum()
}

fn source_manifest_root_count(root_modules: &[SourceManifestRoots]) -> usize {
    root_modules
        .iter()
        .map(|roots| roots.module.roots.len())
        .sum()
}

fn has_source_manifest_roots(config: &dto_bindgen::config::Config) -> bool {
    config.root_discovery.mode == dto_bindgen::config::RootDiscoveryMode::SourceManifest
        || config.packages.iter().any(|package| {
            package.root_discovery.mode == dto_bindgen::config::RootDiscoveryMode::SourceManifest
        })
}

fn root_discovery_mode_name(mode: dto_bindgen::config::RootDiscoveryMode) -> &'static str {
    match mode {
        dto_bindgen::config::RootDiscoveryMode::Explicit => "explicit",
        dto_bindgen::config::RootDiscoveryMode::SourceManifest => "source_manifest",
    }
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
            "hint: generate source-manifest root modules with `dto_bindgen roots --config {}` before compiled exports",
            options.config_path.display()
        )
        .expect("writing to a String cannot fail");
    }
    2
}

fn report_source_manifest_required(options: &CliOptions, stderr: &mut String) -> i32 {
    if options.json {
        writeln!(
            stderr,
            "{{\"error\":\"source_manifest_required\",\"command\":\"{}\",\"config\":\"{}\"}}",
            options.command.as_str(),
            escape_json(&options.config_path.display().to_string())
        )
        .expect("writing to a String cannot fail");
    } else {
        writeln!(
            stderr,
            "error: dto_bindgen {} requires top-level or package root_discovery.mode = \"source_manifest\"",
            options.command.as_str()
        )
        .expect("writing to a String cannot fail");
        writeln!(
            stderr,
            "hint: use dto_bindgen export/check from a compiled root harness for backend output"
        )
        .expect("writing to a String cannot fail");
    }
    2
}

fn output_root(config_path: &std::path::Path, out_dir: &str) -> PathBuf {
    config_dir(config_path).join(out_dir)
}

fn config_dir(config_path: &Path) -> &Path {
    config_path.parent().unwrap_or_else(|| Path::new("."))
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
            other if other.is_control() => {
                write!(escaped, "\\u{:04x}", other as u32)
                    .expect("writing to a String cannot fail");
            }
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
        assert!(stdout.contains("source manifests"));
        assert!(stdout.contains("dto_bindgen roots"));
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
    fn parses_root_module_commands() {
        let roots = parse_args(&["roots".to_owned(), "--config=custom.toml".to_owned()]).unwrap();
        let roots_check =
            parse_args(&["roots-check".to_owned(), "--config=custom.toml".to_owned()]).unwrap();

        assert_eq!(roots.command, Command::Roots);
        assert_eq!(roots_check.command, Command::RootsCheck);
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
    fn export_command_with_source_manifest_still_requires_compiled_roots() {
        let root = temp_project();
        let config_path = write_source_manifest_project(&root);
        let generated_roots = root.join("src/generated/dto_roots.rs");
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "export".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(stderr.contains("requires explicit root descriptors"));
        assert!(stderr.contains("dto_bindgen roots"));
        assert!(!generated_roots.exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn roots_command_writes_source_manifest_root_module() {
        let root = temp_project();
        let config_path = write_source_manifest_project(&root);
        let generated_roots = root.join("src/generated/dto_roots.rs");
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "roots".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("dto_bindgen roots ok"));
        assert!(stdout.contains("roots = 1"));
        let contents = std::fs::read_to_string(&generated_roots).unwrap();
        assert!(
            contents.contains(
                "::dto_bindgen::export::RootDescriptor::new::<crate::sdk::UserProfile>()"
            )
        );
        assert!(!contents.contains("InternalState"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn roots_check_command_reports_missing_source_manifest_root_module_without_writing() {
        let root = temp_project();
        let config_path = write_source_manifest_project(&root);
        let generated_roots = root.join("src/generated/dto_roots.rs");
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "roots-check".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert!(stderr.contains("generated root module is missing"));
        assert!(!generated_roots.exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn roots_check_command_accepts_current_source_manifest_root_module() {
        let root = temp_project();
        let config_path = write_source_manifest_project(&root);
        let mut stdout = String::new();
        let mut stderr = String::new();

        assert_eq!(
            run(
                vec![
                    "roots".to_owned(),
                    "--config".to_owned(),
                    config_path.display().to_string(),
                ],
                &mut stdout,
                &mut stderr,
            ),
            0
        );

        stdout.clear();
        stderr.clear();
        let code = run(
            vec![
                "roots-check".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("dto_bindgen roots ok"));

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
  "schema_version": 1,
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

    #[test]
    fn diagnostics_command_reports_source_manifest_json() {
        let root = temp_project();
        let config_path = write_source_manifest_project(&root);
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

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("\"mode\":\"source_manifest\""));
        assert!(stdout.contains("\"root_module_count\":1"));
        assert!(stdout.contains("\"source_file_count\":1"));
        assert!(stdout.contains("\"root_count\":1"));
        assert!(stdout.contains("\"status\":\"missing\""));
        assert!(stdout.contains("\"source_files\":[\"src/sdk.rs\"]"));
        assert!(stdout.contains("\"rust_name\":\"UserProfile\""));
        assert!(stdout.contains("\"type_path\":\"crate::sdk::UserProfile\""));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diagnostics_command_reports_current_source_manifest_json() {
        let root = temp_project();
        let config_path = write_source_manifest_project(&root);
        let mut stdout = String::new();
        let mut stderr = String::new();

        assert_eq!(
            run(
                vec![
                    "roots".to_owned(),
                    "--config".to_owned(),
                    config_path.display().to_string(),
                ],
                &mut stdout,
                &mut stderr,
            ),
            0
        );

        stdout.clear();
        stderr.clear();
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

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("\"status\":\"current\""));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diagnostics_command_reports_stale_source_manifest_json() {
        let root = temp_project();
        let config_path = write_source_manifest_project(&root);
        let generated_dir = root.join("src/generated");
        std::fs::create_dir_all(&generated_dir).unwrap();
        std::fs::write(generated_dir.join("dto_roots.rs"), "stale\n").unwrap();
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

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("\"status\":\"stale\""));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn roots_command_writes_package_source_manifest_root_modules() {
        let root = temp_project();
        let config_path = write_package_source_manifest_project(&root);
        let generated_roots = root.join("crates/core/src/generated/dto_roots.rs");
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = run(
            vec![
                "roots".to_owned(),
                "--config".to_owned(),
                config_path.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("dto_bindgen roots ok"));
        assert!(stdout.contains("root_modules = 1"));
        assert!(generated_roots.exists());

        stdout.clear();
        stderr.clear();
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

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("\"package\":\"core\""));
        assert!(stdout.contains("\"status\":\"current\""));

        std::fs::remove_dir_all(root).unwrap();
    }

    fn write_source_manifest_project(root: &Path) -> PathBuf {
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("sdk.rs"),
            r#"
#[derive(Dto)]
#[dto(export)]
struct UserProfile {
    id: String,
    internal: InternalState,
}

#[derive(Dto)]
struct InternalState {
    id: String,
}
"#,
        )
        .unwrap();
        let config_path = root.join("dto_bindgen.toml");
        std::fs::write(
            &config_path,
            r#"
[root_discovery]
mode = "source_manifest"
source_files = ["src/sdk.rs"]
root_module_file = "src/generated/dto_roots.rs"
"#,
        )
        .unwrap();
        config_path
    }

    fn write_package_source_manifest_project(root: &Path) -> PathBuf {
        let src = root.join("crates/core/src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("sdk.rs"),
            r#"
#[derive(Dto)]
#[dto(export)]
struct CoreProfile {
    id: String,
}
"#,
        )
        .unwrap();
        let config_path = root.join("dto_bindgen.toml");
        std::fs::write(
            &config_path,
            r#"
[[package]]
key = "core"
rust_package = "radroots-core"
rust_crate = "radroots_core"
npm = "@radroots/core-bindings"
out_dir = "packages/core-bindings/src/generated"

[package.root_discovery]
mode = "source_manifest"
source_files = ["crates/core/src/sdk.rs"]
root_module_file = "crates/core/src/generated/dto_roots.rs"
"#,
        )
        .unwrap();
        config_path
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
