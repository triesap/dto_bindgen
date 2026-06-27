use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::{
    Attribute, Expr, ExprLit, Fields, GenericArgument, Item, ItemEnum, ItemStruct, Lit, Meta,
    PathArguments, Type, parse::Parser, punctuated::Punctuated, spanned::Spanned,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InventoryManifest {
    pub sdk: InventoryManifestSdk,
    pub roots: Vec<String>,
    pub typescript: InventoryManifestTarget,
    pub python: InventoryManifestTarget,
}

impl InventoryManifest {
    pub fn from_toml_str(input: &str) -> Result<Self, InventoryManifestError> {
        toml::from_str(input).map_err(|source| InventoryManifestError {
            message: source.to_string(),
        })
    }

    pub fn from_toml_path(
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, InventoryManifestError> {
        let input =
            std::fs::read_to_string(path.as_ref()).map_err(|source| InventoryManifestError {
                message: source.to_string(),
            })?;
        Self::from_toml_str(&input)
    }
}

impl Default for InventoryManifest {
    fn default() -> Self {
        Self {
            sdk: InventoryManifestSdk::default(),
            roots: Vec::new(),
            typescript: InventoryManifestTarget {
                package_shape: BTreeMap::from([
                    ("emit".to_owned(), "ts".to_owned()),
                    ("module_resolution".to_owned(), "bundler".to_owned()),
                ]),
                generated_artifact_policy: "unknown".to_owned(),
            },
            python: InventoryManifestTarget {
                package_shape: BTreeMap::new(),
                generated_artifact_policy: "unknown".to_owned(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InventoryManifestSdk {
    pub root: String,
    pub package: String,
    pub source_files: Vec<String>,
}

impl Default for InventoryManifestSdk {
    fn default() -> Self {
        Self {
            root: ".".to_owned(),
            package: String::new(),
            source_files: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InventoryManifestTarget {
    pub package_shape: BTreeMap<String, String>,
    pub generated_artifact_policy: String,
}

impl Default for InventoryManifestTarget {
    fn default() -> Self {
        Self {
            package_shape: BTreeMap::new(),
            generated_artifact_policy: "unknown".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryManifestError {
    pub message: String,
}

impl fmt::Display for InventoryManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to load inventory manifest: {}", self.message)
    }
}

impl std::error::Error for InventoryManifestError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryReport {
    pub schema_version: u32,
    pub generator: String,
    pub sdk: InventorySdkReport,
    pub roots: Vec<String>,
    pub serde: InventorySerdeReport,
    pub types: InventoryTypesReport,
    pub typescript: InventoryTargetReport,
    pub python: InventoryTargetReport,
    pub diagnostics: Vec<InventoryFinding>,
    pub promotions: InventoryPromotions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySdkReport {
    pub root: String,
    pub package: String,
    pub source_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InventorySerdeReport {
    pub supported_attrs: Vec<InventoryAttrUsage>,
    pub unsupported_attrs: Vec<InventoryAttrUsage>,
    pub default_usage: Vec<InventoryAttrUsage>,
    pub skipped_fields: Vec<InventoryFieldUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InventoryTypesReport {
    pub large_integer_fields: Vec<InventoryFieldUsage>,
    pub third_party_fields: Vec<InventoryFieldUsage>,
    pub custom_fields_without_dto: Vec<InventoryFieldUsage>,
    pub generic_dtos: Vec<String>,
    pub unsupported_shapes: Vec<InventoryFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryTargetReport {
    pub package_shape: BTreeMap<String, String>,
    pub generated_artifact_policy: String,
    pub ts_rs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InventoryPromotions {
    pub required: Vec<PromotionDecision>,
    pub deferred: Vec<PromotionDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PromotionDecision {
    pub feature: String,
    pub decision: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryAttrUsage {
    pub location: InventoryLocation,
    pub type_name: String,
    pub field_name: Option<String>,
    pub variant_name: Option<String>,
    pub namespace: String,
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryFieldUsage {
    pub location: InventoryLocation,
    pub type_name: String,
    pub field_name: String,
    pub variant_name: Option<String>,
    pub rust_type: String,
    pub evidence: String,
}

pub fn build_inventory_report(
    manifest: InventoryManifest,
    inventories: Vec<SourceInventory>,
) -> InventoryReport {
    let roots = sorted_unique(manifest.roots);
    let mut sdk_source_files = manifest.sdk.source_files.clone();
    if sdk_source_files.is_empty() {
        sdk_source_files = inventories
            .iter()
            .map(|inventory| inventory.source_file.clone())
            .collect();
    }
    sdk_source_files.sort();
    sdk_source_files.dedup();

    let mut report = InventoryReport {
        schema_version: 1,
        generator: "dto_bindgen".to_owned(),
        sdk: InventorySdkReport {
            root: manifest.sdk.root,
            package: manifest.sdk.package,
            source_files: sdk_source_files,
        },
        roots,
        serde: InventorySerdeReport::default(),
        types: InventoryTypesReport::default(),
        typescript: InventoryTargetReport {
            package_shape: manifest.typescript.package_shape,
            generated_artifact_policy: manifest.typescript.generated_artifact_policy,
            ts_rs: BTreeMap::new(),
        },
        python: InventoryTargetReport {
            package_shape: manifest.python.package_shape,
            generated_artifact_policy: manifest.python.generated_artifact_policy,
            ts_rs: BTreeMap::new(),
        },
        diagnostics: Vec::new(),
        promotions: InventoryPromotions::default(),
    };

    for inventory in inventories {
        report.diagnostics.extend(inventory.findings.clone());
        collect_report_items(&inventory, &mut report);
    }

    sort_report(&mut report);
    report.promotions = promotion_decisions(&report);
    report
}

pub fn render_inventory_json(report: &InventoryReport) -> Result<String, serde_json::Error> {
    let mut output = serde_json::to_string_pretty(report)?;
    output.push('\n');
    Ok(output)
}

pub fn render_inventory_markdown(report: &InventoryReport) -> String {
    let mut output = String::new();

    output.push_str("# SDK Inventory Pilot Report\n\n");
    output.push_str("## SDK\n\n");
    output.push_str(&format!("- Root: `{}`\n", report.sdk.root));
    output.push_str(&format!("- Package: `{}`\n", report.sdk.package));
    output.push_str(&format!(
        "- Source files: {}\n\n",
        report.sdk.source_files.len()
    ));

    output.push_str("## Roots\n\n");
    if report.roots.is_empty() {
        output.push_str("- No explicit roots declared.\n\n");
    } else {
        for root in &report.roots {
            output.push_str(&format!("- `{root}`\n"));
        }
        output.push('\n');
    }

    output.push_str("## Serde\n\n");
    output.push_str(&format!(
        "- Supported attrs: {}\n",
        report.serde.supported_attrs.len()
    ));
    output.push_str(&format!(
        "- Unsupported attrs: {}\n",
        report.serde.unsupported_attrs.len()
    ));
    output.push_str(&format!(
        "- Defaults: {}\n",
        report.serde.default_usage.len()
    ));
    output.push_str(&format!(
        "- Skipped fields: {}\n\n",
        report.serde.skipped_fields.len()
    ));

    output.push_str("## Types\n\n");
    output.push_str(&format!(
        "- Large integer fields: {}\n",
        report.types.large_integer_fields.len()
    ));
    output.push_str(&format!(
        "- Third-party fields: {}\n",
        report.types.third_party_fields.len()
    ));
    output.push_str(&format!(
        "- Custom field candidates: {}\n",
        report.types.custom_fields_without_dto.len()
    ));
    output.push_str(&format!(
        "- Generic DTOs: {}\n",
        report.types.generic_dtos.len()
    ));
    output.push_str(&format!(
        "- Unsupported shapes: {}\n\n",
        report.types.unsupported_shapes.len()
    ));

    output.push_str("## TypeScript\n\n");
    output.push_str(&format!(
        "- Artifact policy: `{}`\n",
        report.typescript.generated_artifact_policy
    ));
    output.push_str(&format!(
        "- Package shape keys: {}\n\n",
        report.typescript.package_shape.len()
    ));

    output.push_str("## Python\n\n");
    output.push_str(&format!(
        "- Artifact policy: `{}`\n",
        report.python.generated_artifact_policy
    ));
    output.push_str(&format!(
        "- Package shape keys: {}\n\n",
        report.python.package_shape.len()
    ));

    output.push_str("## Diagnostics\n\n");
    if report.diagnostics.is_empty() {
        output.push_str("- No inventory diagnostics.\n\n");
    } else {
        for diagnostic in &report.diagnostics {
            output.push_str(&format!(
                "- `{}` {:?}: {} at {}:{}\n",
                diagnostic.code,
                diagnostic.severity,
                diagnostic.message,
                diagnostic.location.file,
                diagnostic.location.line
            ));
        }
        output.push('\n');
    }

    output.push_str("## Promotion Decisions\n\n");
    output.push_str("| Feature | Decision | Evidence |\n");
    output.push_str("|---|---|---|\n");
    if report.promotions.required.is_empty() && report.promotions.deferred.is_empty() {
        output.push_str("| none | deferred | no inventory evidence |\n");
    } else {
        for decision in report
            .promotions
            .required
            .iter()
            .chain(report.promotions.deferred.iter())
        {
            output.push_str(&format!(
                "| {} | {} | {} |\n",
                markdown_cell(&decision.feature),
                markdown_cell(&decision.decision),
                markdown_cell(&decision.evidence)
            ));
        }
    }

    output
}

fn collect_report_items(inventory: &SourceInventory, report: &mut InventoryReport) {
    for item in &inventory.items {
        for attr in &item.attrs {
            let usage = attr_usage(item, None, None, attr);
            if attr.supported {
                report.serde.supported_attrs.push(usage.clone());
            } else {
                report.serde.unsupported_attrs.push(usage.clone());
            }
            if attr.namespace == "serde" && attr.name == "default" {
                report.serde.default_usage.push(usage);
            }
        }

        if !item.generics.is_empty() {
            report.types.generic_dtos.push(item.rust_name.clone());
        }

        for field in &item.fields {
            collect_field_usage(&item.rust_name, None, field, report);
        }

        for variant in &item.variants {
            for attr in &variant.attrs {
                let usage = attr_usage(item, Some(variant), None, attr);
                if attr.supported {
                    report.serde.supported_attrs.push(usage);
                } else {
                    report.serde.unsupported_attrs.push(usage);
                }
            }

            for field in &variant.fields {
                collect_field_usage(&item.rust_name, Some(&variant.rust_name), field, report);
            }
        }
    }

    report.types.unsupported_shapes.extend(
        inventory
            .findings
            .iter()
            .filter(|finding| {
                matches!(
                    finding.code.as_str(),
                    "INV1002" | "INV1003" | "INV1004" | "INV1005"
                )
            })
            .cloned(),
    );
}

fn collect_field_usage(
    type_name: &str,
    variant_name: Option<&str>,
    field: &InventoryField,
    report: &mut InventoryReport,
) {
    for attr in &field.attrs {
        let usage = InventoryAttrUsage {
            location: attr.location.clone(),
            type_name: type_name.to_owned(),
            field_name: Some(field.rust_name.clone()),
            variant_name: variant_name.map(str::to_owned),
            namespace: attr.namespace.clone(),
            name: attr.name.clone(),
            value: attr.value.clone(),
        };
        if attr.supported {
            report.serde.supported_attrs.push(usage.clone());
        } else {
            report.serde.unsupported_attrs.push(usage.clone());
        }
        if attr.namespace == "serde" && attr.name == "default" {
            report.serde.default_usage.push(usage);
        }
    }

    if field.skipped {
        report.serde.skipped_fields.push(field_usage(
            type_name,
            variant_name,
            field,
            "field is skipped by serde or dto metadata",
        ));
    }

    for class in &field.classes {
        match class {
            InventoryTypeClass::LargeInteger { rust_type } => {
                report.types.large_integer_fields.push(field_usage(
                    type_name,
                    variant_name,
                    field,
                    &format!("large integer `{rust_type}` requires explicit numeric policy"),
                ));
            }
            InventoryTypeClass::ThirdParty { family, rust_type } => {
                report.types.third_party_fields.push(field_usage(
                    type_name,
                    variant_name,
                    field,
                    &format!("third-party `{rust_type}` from `{family}` requires promotion review"),
                ));
            }
            InventoryTypeClass::CustomCandidate { rust_type } => {
                report.types.custom_fields_without_dto.push(field_usage(
                    type_name,
                    variant_name,
                    field,
                    &format!("custom candidate `{rust_type}` may need a `Dto` descriptor"),
                ));
            }
        }
    }
}

fn attr_usage(
    item: &InventoryItem,
    variant: Option<&InventoryVariant>,
    field: Option<&InventoryField>,
    attr: &InventoryAttribute,
) -> InventoryAttrUsage {
    InventoryAttrUsage {
        location: attr.location.clone(),
        type_name: item.rust_name.clone(),
        field_name: field.map(|field| field.rust_name.clone()),
        variant_name: variant.map(|variant| variant.rust_name.clone()),
        namespace: attr.namespace.clone(),
        name: attr.name.clone(),
        value: attr.value.clone(),
    }
}

fn field_usage(
    type_name: &str,
    variant_name: Option<&str>,
    field: &InventoryField,
    evidence: &str,
) -> InventoryFieldUsage {
    InventoryFieldUsage {
        location: field.location.clone(),
        type_name: type_name.to_owned(),
        field_name: field.rust_name.clone(),
        variant_name: variant_name.map(str::to_owned),
        rust_type: field.type_name.clone(),
        evidence: evidence.to_owned(),
    }
}

fn sort_report(report: &mut InventoryReport) {
    report.serde.supported_attrs.sort_by(attr_usage_order);
    report.serde.supported_attrs.dedup();
    report.serde.unsupported_attrs.sort_by(attr_usage_order);
    report.serde.unsupported_attrs.dedup();
    report.serde.default_usage.sort_by(attr_usage_order);
    report.serde.default_usage.dedup();
    report.serde.skipped_fields.sort_by(field_usage_order);
    report.serde.skipped_fields.dedup();
    report.types.large_integer_fields.sort_by(field_usage_order);
    report.types.large_integer_fields.dedup();
    report.types.third_party_fields.sort_by(field_usage_order);
    report.types.third_party_fields.dedup();
    report
        .types
        .custom_fields_without_dto
        .sort_by(field_usage_order);
    report.types.custom_fields_without_dto.dedup();
    report.types.generic_dtos.sort();
    report.types.generic_dtos.dedup();
    report.types.unsupported_shapes.sort_by(finding_order);
    report.types.unsupported_shapes.dedup();
    report.diagnostics.sort_by(finding_order);
    report.diagnostics.dedup();
}

fn attr_usage_order(left: &InventoryAttrUsage, right: &InventoryAttrUsage) -> std::cmp::Ordering {
    left.location
        .file
        .cmp(&right.location.file)
        .then_with(|| left.location.line.cmp(&right.location.line))
        .then_with(|| left.location.column.cmp(&right.location.column))
        .then_with(|| left.type_name.cmp(&right.type_name))
        .then_with(|| left.variant_name.cmp(&right.variant_name))
        .then_with(|| left.field_name.cmp(&right.field_name))
        .then_with(|| left.namespace.cmp(&right.namespace))
        .then_with(|| left.name.cmp(&right.name))
}

fn field_usage_order(
    left: &InventoryFieldUsage,
    right: &InventoryFieldUsage,
) -> std::cmp::Ordering {
    left.location
        .file
        .cmp(&right.location.file)
        .then_with(|| left.location.line.cmp(&right.location.line))
        .then_with(|| left.location.column.cmp(&right.location.column))
        .then_with(|| left.type_name.cmp(&right.type_name))
        .then_with(|| left.variant_name.cmp(&right.variant_name))
        .then_with(|| left.field_name.cmp(&right.field_name))
        .then_with(|| left.rust_type.cmp(&right.rust_type))
}

fn finding_order(left: &InventoryFinding, right: &InventoryFinding) -> std::cmp::Ordering {
    left.location
        .file
        .cmp(&right.location.file)
        .then_with(|| left.location.line.cmp(&right.location.line))
        .then_with(|| left.location.column.cmp(&right.location.column))
        .then_with(|| left.code.cmp(&right.code))
        .then_with(|| left.type_name.cmp(&right.type_name))
        .then_with(|| left.variant_name.cmp(&right.variant_name))
        .then_with(|| left.field_name.cmp(&right.field_name))
        .then_with(|| left.attribute.cmp(&right.attribute))
}

fn promotion_decisions(report: &InventoryReport) -> InventoryPromotions {
    let roots = report
        .roots
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut required = BTreeSet::new();
    let mut deferred = BTreeSet::new();

    for attr in &report.serde.unsupported_attrs {
        let decision = PromotionDecision {
            feature: format!("{}::{}", attr.namespace, attr.name),
            decision: if roots.contains(attr.type_name.as_str()) {
                "required_promotion"
            } else {
                "deferred_until_required"
            }
            .to_owned(),
            evidence: format!("{}:{}", attr.location.file, attr.location.line),
        };
        if roots.contains(attr.type_name.as_str()) {
            required.insert(decision);
        } else {
            deferred.insert(decision);
        }
    }
    for field in &report.types.large_integer_fields {
        let decision = PromotionDecision {
            feature: format!("large_integer_policy:{}", field.rust_type),
            decision: if roots.contains(field.type_name.as_str()) {
                "required_numeric_policy"
            } else {
                "deferred_until_required"
            }
            .to_owned(),
            evidence: format!("{}::{}", field.type_name, field.field_name),
        };
        if roots.contains(field.type_name.as_str()) {
            required.insert(decision);
        } else {
            deferred.insert(decision);
        }
    }
    for field in &report.types.third_party_fields {
        let decision = PromotionDecision {
            feature: format!("third_party_type:{}", field.rust_type),
            decision: if roots.contains(field.type_name.as_str()) {
                "required_mapping_review"
            } else {
                "deferred_until_required"
            }
            .to_owned(),
            evidence: format!("{}::{}", field.type_name, field.field_name),
        };
        if roots.contains(field.type_name.as_str()) {
            required.insert(decision);
        } else {
            deferred.insert(decision);
        }
    }
    for field in &report.types.custom_fields_without_dto {
        let decision = PromotionDecision {
            feature: format!("custom_type:{}", field.rust_type),
            decision: if roots.contains(field.type_name.as_str()) {
                "required_descriptor_review"
            } else {
                "review_descriptor"
            }
            .to_owned(),
            evidence: format!("{}::{}", field.type_name, field.field_name),
        };
        if roots.contains(field.type_name.as_str()) {
            required.insert(decision);
        } else {
            deferred.insert(decision);
        }
    }
    for type_name in &report.types.generic_dtos {
        let decision = PromotionDecision {
            feature: "generic_dtos".to_owned(),
            decision: if roots.contains(type_name.as_str()) {
                "required_promotion"
            } else {
                "deferred_until_required"
            }
            .to_owned(),
            evidence: type_name.clone(),
        };
        if roots.contains(type_name.as_str()) {
            required.insert(decision);
        } else {
            deferred.insert(decision);
        }
    }

    InventoryPromotions {
        required: required.into_iter().collect(),
        deferred: deferred.into_iter().collect(),
    }
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInventory {
    pub source_file: String,
    pub items: Vec<InventoryItem>,
    pub findings: Vec<InventoryFinding>,
}

impl SourceInventory {
    pub fn new(source_file: impl Into<String>) -> Self {
        Self {
            source_file: source_file.into(),
            items: Vec::new(),
            findings: Vec::new(),
        }
    }

    pub fn fields(&self) -> impl Iterator<Item = &InventoryField> {
        self.items.iter().flat_map(|item| {
            item.fields
                .iter()
                .chain(item.variants.iter().flat_map(|v| &v.fields))
        })
    }

    pub fn exported_roots(&self) -> impl Iterator<Item = &InventoryItem> {
        self.items.iter().filter(|item| item.exported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryItem {
    pub kind: InventoryItemKind,
    pub rust_name: String,
    pub exported: bool,
    pub derives: Vec<String>,
    pub generics: Vec<String>,
    pub attrs: Vec<InventoryAttribute>,
    pub fields: Vec<InventoryField>,
    pub variants: Vec<InventoryVariant>,
    pub location: InventoryLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryItemKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryVariant {
    pub rust_name: String,
    pub attrs: Vec<InventoryAttribute>,
    pub fields: Vec<InventoryField>,
    pub location: InventoryLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryField {
    pub rust_name: String,
    pub type_name: String,
    pub type_paths: Vec<String>,
    pub attrs: Vec<InventoryAttribute>,
    pub skipped: bool,
    pub classes: Vec<InventoryTypeClass>,
    pub location: InventoryLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryAttribute {
    pub namespace: String,
    pub name: String,
    pub value: Option<String>,
    pub supported: bool,
    pub location: InventoryLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryTypeClass {
    LargeInteger { rust_type: String },
    ThirdParty { family: String, rust_type: String },
    CustomCandidate { rust_type: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryFinding {
    pub code: String,
    pub severity: InventorySeverity,
    pub message: String,
    pub location: InventoryLocation,
    pub type_name: Option<String>,
    pub field_name: Option<String>,
    pub variant_name: Option<String>,
    pub attribute: Option<String>,
    pub help: Option<String>,
}

impl InventoryFinding {
    fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        location: InventoryLocation,
    ) -> Self {
        Self {
            code: code.into(),
            severity: InventorySeverity::Warning,
            message: message.into(),
            location,
            type_name: None,
            field_name: None,
            variant_name: None,
            attribute: None,
            help: None,
        }
    }

    fn with_type(mut self, type_name: impl Into<String>) -> Self {
        self.type_name = Some(type_name.into());
        self
    }

    fn with_field(mut self, field_name: impl Into<String>) -> Self {
        self.field_name = Some(field_name.into());
        self
    }

    fn with_variant(mut self, variant_name: impl Into<String>) -> Self {
        self.variant_name = Some(variant_name.into());
        self
    }

    fn with_attribute(mut self, attribute: impl Into<String>) -> Self {
        self.attribute = Some(attribute.into());
        self
    }

    fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventorySeverity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl InventoryLocation {
    pub fn new(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryScanError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for InventoryScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to parse Rust source `{}` for inventory: {}",
            self.path, self.message
        )
    }
}

impl std::error::Error for InventoryScanError {}

pub fn scan_rust_source(
    path: impl Into<String>,
    input: &str,
) -> Result<SourceInventory, InventoryScanError> {
    let path = path.into();
    let file = syn::parse_file(input).map_err(|source| InventoryScanError {
        path: path.clone(),
        message: source.to_string(),
    })?;
    let known_items = collect_known_items(&file.items);
    let known_dto_items = collect_known_dto_items(&file.items);
    let mut scanner = InventoryScanner {
        path: path.clone(),
        known_items,
        known_dto_items,
        inventory: SourceInventory::new(path),
    };

    for item in &file.items {
        scanner.scan_item(item);
    }

    scanner.inventory.items.sort_by(|left, right| {
        left.location
            .line
            .cmp(&right.location.line)
            .then_with(|| left.rust_name.cmp(&right.rust_name))
    });
    scanner.inventory.findings.sort_by(|left, right| {
        left.location
            .file
            .cmp(&right.location.file)
            .then_with(|| left.location.line.cmp(&right.location.line))
            .then_with(|| left.location.column.cmp(&right.location.column))
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.message.cmp(&right.message))
    });

    Ok(scanner.inventory)
}

struct InventoryScanner {
    path: String,
    known_items: BTreeSet<String>,
    known_dto_items: BTreeSet<String>,
    inventory: SourceInventory,
}

impl InventoryScanner {
    fn scan_item(&mut self, item: &Item) {
        match item {
            Item::Struct(item) => self.scan_struct(item),
            Item::Enum(item) => self.scan_enum(item),
            _ => {}
        }
    }

    fn scan_struct(&mut self, item: &ItemStruct) {
        let type_name = item.ident.to_string();
        let derives = derive_names(&item.attrs);
        if !has_inventory_derive(&derives) {
            return;
        }

        let attrs = self.collect_attrs(&item.attrs, AttrScope::Container);
        let generics = generic_names(&item.generics);
        let fields = self.scan_fields(&type_name, None, &item.fields);

        if !generics.is_empty() && derives.iter().any(|name| name == "Dto") {
            self.push_finding(
                InventoryFinding::warning(
                    "INV1001",
                    "generic DTO declarations are not supported by `Dto` derive",
                    self.location(item),
                )
                .with_type(type_name.clone())
                .with_help("Keep this deferred unless the real SDK requires generic DTO support."),
            );
        }

        match &item.fields {
            Fields::Named(_) => {}
            Fields::Unnamed(_) => self.push_finding(
                InventoryFinding::warning(
                    "INV1002",
                    "tuple structs are unsupported DTO shapes",
                    self.location(item),
                )
                .with_type(type_name.clone()),
            ),
            Fields::Unit => self.push_finding(
                InventoryFinding::warning(
                    "INV1003",
                    "unit structs are unsupported DTO shapes",
                    self.location(item),
                )
                .with_type(type_name.clone()),
            ),
        }

        self.inventory.items.push(InventoryItem {
            kind: InventoryItemKind::Struct,
            rust_name: type_name,
            exported: has_dto_export(&attrs),
            derives,
            generics,
            attrs,
            fields,
            variants: Vec::new(),
            location: self.location(item),
        });
    }

    fn scan_enum(&mut self, item: &ItemEnum) {
        let type_name = item.ident.to_string();
        let derives = derive_names(&item.attrs);
        if !has_inventory_derive(&derives) {
            return;
        }

        let attrs = self.collect_attrs(&item.attrs, AttrScope::Container);
        let generics = generic_names(&item.generics);
        let tagged = attrs.iter().any(|attr| {
            attr.namespace == "serde" && (attr.name == "tag" || attr.name == "content")
        });
        let mut variants = Vec::new();

        if !generics.is_empty() && derives.iter().any(|name| name == "Dto") {
            self.push_finding(
                InventoryFinding::warning(
                    "INV1001",
                    "generic DTO declarations are not supported by `Dto` derive",
                    self.location(item),
                )
                .with_type(type_name.clone()),
            );
        }

        for variant in &item.variants {
            let variant_name = variant.ident.to_string();
            let variant_attrs = self.collect_attrs(&variant.attrs, AttrScope::Variant);
            let fields = self.scan_fields(&type_name, Some(&variant_name), &variant.fields);

            match &variant.fields {
                Fields::Unit => {}
                Fields::Named(_) if tagged => {}
                Fields::Named(_) => self.push_finding(
                    InventoryFinding::warning(
                        "INV1004",
                        "externally tagged data enum variants are deferred",
                        self.location(variant),
                    )
                    .with_type(type_name.clone())
                    .with_variant(variant_name.clone())
                    .with_help("Add explicit enum tagging or defer until Python representation is specified."),
                ),
                Fields::Unnamed(_) => self.push_finding(
                    InventoryFinding::warning(
                        "INV1005",
                        "tuple enum variants are unsupported DTO shapes",
                        self.location(variant),
                    )
                    .with_type(type_name.clone())
                    .with_variant(variant_name.clone()),
                ),
            }

            variants.push(InventoryVariant {
                rust_name: variant_name,
                attrs: variant_attrs,
                fields,
                location: self.location(variant),
            });
        }

        self.inventory.items.push(InventoryItem {
            kind: InventoryItemKind::Enum,
            rust_name: type_name,
            exported: has_dto_export(&attrs),
            derives,
            generics,
            attrs,
            fields: Vec::new(),
            variants,
            location: self.location(item),
        });
    }

    fn scan_fields(
        &mut self,
        type_name: &str,
        variant_name: Option<&str>,
        fields: &Fields,
    ) -> Vec<InventoryField> {
        let Fields::Named(fields) = fields else {
            return Vec::new();
        };

        fields
            .named
            .iter()
            .filter_map(|field| {
                let ident = field.ident.as_ref()?;
                let field_name = ident.to_string();
                let attrs = self.collect_attrs(&field.attrs, AttrScope::Field);
                let skipped = attrs.iter().any(|attr| {
                    (attr.namespace == "serde" || attr.namespace == "dto") && attr.name == "skip"
                });
                let type_paths = type_paths(&field.ty);
                let mut classes =
                    classify_field_type(&type_paths, &self.known_items, &self.known_dto_items);
                classes.sort();
                classes.dedup();

                for class in &classes {
                    self.push_type_class_finding(
                        type_name,
                        variant_name,
                        &field_name,
                        field,
                        class,
                    );
                }

                Some(InventoryField {
                    rust_name: field_name,
                    type_name: field.ty.to_token_stream().to_string(),
                    type_paths,
                    attrs,
                    skipped,
                    classes,
                    location: self.location(field),
                })
            })
            .collect()
    }

    fn collect_attrs(&mut self, attrs: &[Attribute], scope: AttrScope) -> Vec<InventoryAttribute> {
        let mut output = Vec::new();
        for attr in attrs {
            let Some(namespace) = attr_namespace(attr) else {
                continue;
            };
            let location = self.location(attr);

            match meta_children(&attr.meta) {
                Ok(children) if !children.is_empty() => {
                    for meta in children {
                        let name = meta_path(&meta);
                        let value = meta_value(&meta);
                        let supported = attr_supported(namespace, scope, &meta);
                        let inventory_attr = InventoryAttribute {
                            namespace: namespace.to_owned(),
                            name,
                            value,
                            supported,
                            location: location.clone(),
                        };
                        self.push_attr_finding(scope, &inventory_attr);
                        output.push(inventory_attr);
                    }
                }
                _ => {
                    let name = attr
                        .path()
                        .segments
                        .last()
                        .map(|segment| segment.ident.to_string())
                        .unwrap_or_else(|| namespace.to_owned());
                    let inventory_attr = InventoryAttribute {
                        namespace: namespace.to_owned(),
                        name,
                        value: None,
                        supported: false,
                        location,
                    };
                    self.push_attr_finding(scope, &inventory_attr);
                    output.push(inventory_attr);
                }
            }
        }
        output.sort_by(|left, right| {
            left.namespace
                .cmp(&right.namespace)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.value.cmp(&right.value))
        });
        output
    }

    fn push_attr_finding(&mut self, scope: AttrScope, attr: &InventoryAttribute) {
        if attr.supported {
            return;
        }

        let namespace = match attr.namespace.as_str() {
            "serde" => "Serde",
            "dto" => "dto",
            _ => attr.namespace.as_str(),
        };
        let scope_name = match scope {
            AttrScope::Container => "container",
            AttrScope::Field => "field",
            AttrScope::Variant => "variant",
        };
        self.push_finding(
            InventoryFinding::warning(
                "INV0300",
                format!(
                    "unsupported {namespace} {scope_name} attribute `{}`",
                    attr.name
                ),
                attr.location.clone(),
            )
            .with_attribute(format!("{}::{}", attr.namespace, attr.name))
            .with_help("Keep this deferred unless SDK inventory proves it is required."),
        );
    }

    fn push_type_class_finding(
        &mut self,
        type_name: &str,
        variant_name: Option<&str>,
        field_name: &str,
        field: &syn::Field,
        class: &InventoryTypeClass,
    ) {
        let finding = match class {
            InventoryTypeClass::LargeInteger { rust_type } => InventoryFinding::warning(
                "INV0400",
                format!("large integer field uses `{rust_type}`"),
                self.location(field),
            )
            .with_help("Add an explicit numeric policy before generated TypeScript is adopted."),
            InventoryTypeClass::ThirdParty { family, rust_type } => InventoryFinding::warning(
                "INV1100",
                format!(
                    "third-party field type `{rust_type}` from `{family}` requires inventory review"
                ),
                self.location(field),
            )
            .with_help(
                "Do not add a mapping until the pilot report proves this field is required.",
            ),
            InventoryTypeClass::CustomCandidate { rust_type } => InventoryFinding::warning(
                "INV1101",
                format!("custom field type `{rust_type}` may require a `Dto` descriptor"),
                self.location(field),
            )
            .with_help(
                "Confirm the referenced type is a DTO dependency or add an explicit override.",
            ),
        };

        let finding = finding
            .with_type(type_name.to_owned())
            .with_field(field_name.to_owned());
        let finding = match variant_name {
            Some(variant_name) => finding.with_variant(variant_name.to_owned()),
            None => finding,
        };
        self.push_finding(finding);
    }

    fn push_finding(&mut self, finding: InventoryFinding) {
        self.inventory.findings.push(finding);
    }

    fn location<T>(&self, node: &T) -> InventoryLocation
    where
        T: Spanned,
    {
        let start = node.span().start();
        InventoryLocation::new(&self.path, start.line as u32, start.column as u32)
    }
}

#[derive(Debug, Clone, Copy)]
enum AttrScope {
    Container,
    Field,
    Variant,
}

fn collect_known_items(items: &[Item]) -> BTreeSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) => Some(item.ident.to_string()),
            Item::Enum(item) => Some(item.ident.to_string()),
            _ => None,
        })
        .collect()
}

fn has_dto_export(attrs: &[InventoryAttribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.namespace == "dto" && attr.name == "export")
}

fn collect_known_dto_items(items: &[Item]) -> BTreeSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) if derive_names(&item.attrs).iter().any(|name| name == "Dto") => {
                Some(item.ident.to_string())
            }
            Item::Enum(item) if derive_names(&item.attrs).iter().any(|name| name == "Dto") => {
                Some(item.ident.to_string())
            }
            _ => None,
        })
        .collect()
}

fn has_inventory_derive(derives: &[String]) -> bool {
    derives.iter().any(|name| {
        matches!(
            name.rsplit("::").next(),
            Some("Serialize" | "Deserialize" | "Dto" | "TS")
        )
    })
}

fn attr_namespace(attr: &Attribute) -> Option<&'static str> {
    if attr.path().is_ident("serde") {
        Some("serde")
    } else if attr.path().is_ident("dto") {
        Some("dto")
    } else {
        None
    }
}

fn derive_names(attrs: &[Attribute]) -> Vec<String> {
    let mut names = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("derive") {
            continue;
        }
        if let Ok(children) = meta_children(&attr.meta) {
            for child in children {
                names.push(meta_path(&child));
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn generic_names(generics: &syn::Generics) -> Vec<String> {
    generics
        .params
        .iter()
        .map(|param| match param {
            syn::GenericParam::Type(param) => param.ident.to_string(),
            syn::GenericParam::Lifetime(param) => param.lifetime.ident.to_string(),
            syn::GenericParam::Const(param) => param.ident.to_string(),
        })
        .collect()
}

fn meta_children(meta: &Meta) -> syn::Result<Vec<Meta>> {
    match meta {
        Meta::List(list) => {
            let parser = Punctuated::<Meta, syn::Token![,]>::parse_terminated;
            let children = match parser.parse2(list.tokens.clone()) {
                Ok(children) => children,
                Err(error) => {
                    let Some(repaired) = repair_meta_keyword_tokens(&list.tokens.to_string())
                    else {
                        return Err(error);
                    };
                    parser.parse_str(&repaired)?
                }
            };
            Ok(children.into_iter().collect())
        }
        Meta::Path(_) | Meta::NameValue(_) => Ok(Vec::new()),
    }
}

fn repair_meta_keyword_tokens(tokens: &str) -> Option<String> {
    let repaired = tokens.replace("as =", "r#as =");
    (repaired != tokens).then_some(repaired)
}

fn meta_path(meta: &Meta) -> String {
    match meta {
        Meta::Path(path)
        | Meta::List(syn::MetaList { path, .. })
        | Meta::NameValue(syn::MetaNameValue { path, .. }) => path
            .segments
            .iter()
            .map(|segment| {
                segment
                    .ident
                    .to_string()
                    .trim_start_matches("r#")
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("::"),
    }
}

fn meta_value(meta: &Meta) -> Option<String> {
    let Meta::NameValue(name_value) = meta else {
        return None;
    };
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = &name_value.value
    else {
        return Some(name_value.value.to_token_stream().to_string());
    };
    Some(value.value())
}

fn attr_supported(namespace: &str, scope: AttrScope, meta: &Meta) -> bool {
    let name = meta_path(meta);
    match (namespace, scope, name.as_str()) {
        (
            "serde",
            AttrScope::Container,
            "rename"
            | "rename_all"
            | "rename_all_fields"
            | "tag"
            | "content"
            | "deny_unknown_fields",
        ) => !matches!(meta, Meta::List(_)),
        ("serde", AttrScope::Container, "transparent") => matches!(meta, Meta::Path(_)),
        ("serde", AttrScope::Field, "rename") => !matches!(meta, Meta::List(_)),
        ("serde", AttrScope::Field, "skip") => matches!(meta, Meta::Path(_)),
        ("serde", AttrScope::Field, "skip_serializing_if") => {
            meta_value(meta).as_deref() == Some("Option::is_none")
        }
        ("serde", AttrScope::Field, "default") => matches!(meta, Meta::Path(_)),
        ("serde", AttrScope::Variant, "rename") => !matches!(meta, Meta::List(_)),
        ("dto", AttrScope::Container, "export") => matches!(meta, Meta::Path(_)),
        ("dto", AttrScope::Container, "as") => {
            matches!(meta_value(meta).as_deref(), Some("string" | "string_enum"))
        }
        ("dto", AttrScope::Container, "ts") => dto_ts_name_supported(meta),
        ("dto", AttrScope::Field, "skip") => matches!(meta, Meta::Path(_)),
        ("dto", AttrScope::Field, "as") => meta_value(meta).as_deref() == Some("string"),
        ("dto", AttrScope::Field, "int" | "int_repr") => matches!(
            meta_value(meta).as_deref(),
            Some("json_string" | "json_number")
        ),
        ("dto", AttrScope::Field, "bytes") => meta_value(meta).as_deref() == Some("base64"),
        ("dto", AttrScope::Variant, "rename") => !matches!(meta, Meta::List(_)),
        _ => false,
    }
}

fn dto_ts_name_supported(meta: &Meta) -> bool {
    let Ok(children) = meta_children(meta) else {
        return false;
    };
    children.len() == 1
        && children.iter().all(|child| {
            meta_path(child) == "name"
                && matches!(child, Meta::NameValue(_))
                && meta_value(child).is_some()
        })
}

fn type_paths(ty: &Type) -> Vec<String> {
    let mut paths = Vec::new();
    collect_type_paths(ty, &mut paths);
    paths.sort();
    paths.dedup();
    paths
}

fn collect_type_paths(ty: &Type, paths: &mut Vec<String>) {
    match ty {
        Type::Array(array) => collect_type_paths(&array.elem, paths),
        Type::Group(group) => collect_type_paths(&group.elem, paths),
        Type::Paren(paren) => collect_type_paths(&paren.elem, paths),
        Type::Path(path) => collect_path(&path.path, paths),
        Type::Reference(reference) => collect_type_paths(&reference.elem, paths),
        Type::Slice(slice) => collect_type_paths(&slice.elem, paths),
        Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                collect_type_paths(elem, paths);
            }
        }
        _ => {}
    }
}

fn collect_path(path: &syn::Path, paths: &mut Vec<String>) {
    let path_name = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    paths.push(path_name);

    for segment in &path.segments {
        let PathArguments::AngleBracketed(args) = &segment.arguments else {
            continue;
        };
        for arg in &args.args {
            if let GenericArgument::Type(ty) = arg {
                collect_type_paths(ty, paths);
            }
        }
    }
}

fn classify_field_type(
    paths: &[String],
    known_items: &BTreeSet<String>,
    known_dto_items: &BTreeSet<String>,
) -> Vec<InventoryTypeClass> {
    let mut classes = Vec::new();
    for path in paths {
        let Some(last) = path.rsplit("::").next() else {
            continue;
        };
        if is_large_integer(last) {
            classes.push(InventoryTypeClass::LargeInteger {
                rust_type: path.clone(),
            });
            continue;
        }
        if let Some(family) = third_party_family(path, last) {
            classes.push(InventoryTypeClass::ThirdParty {
                family,
                rust_type: path.clone(),
            });
            continue;
        }
        if is_builtin_or_container(last) {
            continue;
        }
        if known_dto_items.contains(last) {
            continue;
        }
        if known_items.contains(last) || likely_custom_type(last) {
            classes.push(InventoryTypeClass::CustomCandidate {
                rust_type: path.clone(),
            });
        }
    }
    classes
}

fn is_large_integer(name: &str) -> bool {
    matches!(name, "i64" | "u64" | "i128" | "u128" | "isize" | "usize")
}

fn is_builtin_or_container(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "str"
            | "bool"
            | "i8"
            | "u8"
            | "i16"
            | "u16"
            | "i32"
            | "u32"
            | "f32"
            | "f64"
            | "Option"
            | "Vec"
            | "HashMap"
            | "BTreeMap"
            | "Box"
    )
}

fn third_party_family(path: &str, last: &str) -> Option<String> {
    let mut segments = path.split("::");
    let first = segments.next().unwrap_or(path);
    match first {
        "uuid" => Some("uuid".to_owned()),
        "chrono" => Some("chrono".to_owned()),
        "time" => Some("time".to_owned()),
        "serde_json" => Some("serde_json".to_owned()),
        "url" => Some("url".to_owned()),
        "bytes" => Some("bytes".to_owned()),
        "rust_decimal" => Some("rust_decimal".to_owned()),
        "indexmap" => Some("indexmap".to_owned()),
        "Cow" => Some("cow".to_owned()),
        "std" | "core" if path.contains("borrow::Cow") => Some("cow".to_owned()),
        "NonZeroI8" | "NonZeroU8" | "NonZeroI16" | "NonZeroU16" | "NonZeroI32" | "NonZeroU32"
        | "NonZeroI64" | "NonZeroU64" | "NonZeroI128" | "NonZeroU128" | "NonZeroIsize"
        | "NonZeroUsize" => Some("nonzero".to_owned()),
        "std" | "core" if path.contains("num::NonZero") => Some("nonzero".to_owned()),
        _ if last.starts_with("NonZero") => Some("nonzero".to_owned()),
        _ => None,
    }
}

fn likely_custom_type(name: &str) -> bool {
    name.chars().next().map(char::is_uppercase).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_sdk_like_source_without_requiring_dto_compile_success() {
        let source = r#"
            use serde::{Deserialize, Serialize};

            #[derive(Serialize, Deserialize, Dto)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct UserProfile {
                user_id: uuid::Uuid,
                #[serde(skip)]
                internal_note: String,
                #[serde(flatten)]
                metadata: serde_json::Value,
                balance: u128,
                nested: MissingDto,
            }

            #[derive(Serialize, Dto)]
            struct Wrapper<T> {
                value: T,
            }

            #[derive(Serialize, Deserialize)]
            #[serde(untagged)]
            enum SdkEvent {
                UserCreated { user: UserProfile },
                Other(String),
            }
        "#;

        let inventory = scan_rust_source("src/sdk.rs", source).unwrap();

        assert_eq!(inventory.items.len(), 3);
        assert!(
            inventory
                .findings
                .iter()
                .any(|finding| finding.code == "INV0300"
                    && finding.attribute.as_deref() == Some("serde::flatten"))
        );
        assert!(
            inventory
                .findings
                .iter()
                .any(|finding| finding.code == "INV1001"
                    && finding.type_name.as_deref() == Some("Wrapper"))
        );
        assert!(
            inventory
                .findings
                .iter()
                .any(|finding| finding.code == "INV1005"
                    && finding.variant_name.as_deref() == Some("Other"))
        );

        let internal_note = inventory
            .fields()
            .find(|field| field.rust_name == "internal_note")
            .unwrap();
        assert!(internal_note.skipped);

        let user_id = inventory
            .fields()
            .find(|field| field.rust_name == "user_id")
            .unwrap();
        assert!(user_id.classes.iter().any(|class| {
            matches!(
                class,
                InventoryTypeClass::ThirdParty { family, .. } if family == "uuid"
            )
        }));

        let balance = inventory
            .fields()
            .find(|field| field.rust_name == "balance")
            .unwrap();
        assert!(balance.classes.iter().any(|class| {
            matches!(
                class,
                InventoryTypeClass::LargeInteger { rust_type } if rust_type == "u128"
            )
        }));
    }

    #[test]
    fn reports_container_default_as_unsupported_inventory_usage() {
        let source = r#"
            #[derive(Dto)]
            #[serde(default)]
            struct Defaults {
                tags: Vec<String>,
            }
        "#;

        let inventory = scan_rust_source("src/defaults.rs", source).unwrap();
        let item = inventory.items.first().unwrap();

        assert!(item.attrs.iter().any(|attr| {
            attr.namespace == "serde" && attr.name == "default" && !attr.supported
        }));
        assert!(
            inventory
                .findings
                .iter()
                .any(|finding| { finding.attribute.as_deref() == Some("serde::default") })
        );
    }

    #[test]
    fn scans_dto_export_roots_without_promoting_internal_types() {
        let source = r#"
            #[derive(Dto)]
            #[dto(export)]
            struct UserProfile {
                id: String,
                internal: InternalState,
            }

            #[derive(Dto)]
            struct InternalState {
                #[dto(skip)]
                note: String,
            }

            #[derive(Dto)]
            #[dto(export)]
            enum UserEvent {
                Renamed { profile: UserProfile },
                Deleted,
            }
        "#;

        let inventory = scan_rust_source("src/sdk.rs", source).unwrap();
        let exported = inventory
            .exported_roots()
            .map(|item| item.rust_name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(exported, ["UserProfile", "UserEvent"]);
        assert!(
            !inventory
                .items
                .iter()
                .find(|item| item.rust_name == "InternalState")
                .unwrap()
                .exported
        );
        assert!(inventory.items.iter().any(|item| {
            item.rust_name == "UserProfile"
                && item
                    .attrs
                    .iter()
                    .any(|attr| attr.namespace == "dto" && attr.name == "export" && attr.supported)
        }));
        assert!(
            !inventory
                .findings
                .iter()
                .any(|finding| { finding.attribute.as_deref() == Some("dto::export") })
        );
    }

    #[test]
    fn scans_v1_dto_attrs_as_supported_inventory_input() {
        let source = r#"
            #[derive(Dto)]
            #[dto(as = "string")]
            struct Decimal {
                value: String,
            }

            #[derive(Dto)]
            #[serde(transparent)]
            struct UserId {
                value: String,
            }

            #[derive(Dto)]
            #[dto(as = "string_enum")]
            enum Unit {
                #[dto(rename = "kg")]
                Kilogram,
                #[dto(rename = "lb")]
                Pound,
            }

            #[derive(Dto)]
            #[dto(export)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Attachment {
                #[dto(skip)]
                internal_note: String,
                #[dto(as = "string")]
                amount: Decimal,
                #[dto(int = "json_string")]
                sequence: u128,
                #[dto(int_repr = "json_number")]
                legacy_sequence: u64,
                #[dto(bytes = "base64")]
                payload: Vec<u8>,
            }
        "#;

        let inventory = scan_rust_source("src/sdk.rs", source).unwrap();
        let unsupported_attrs = inventory
            .findings
            .iter()
            .filter(|finding| finding.code == "INV0300")
            .map(|finding| finding.attribute.as_deref().unwrap_or_default())
            .collect::<Vec<_>>();

        assert_eq!(unsupported_attrs, Vec::<&str>::new());
    }

    #[test]
    fn reports_unknown_dto_attr_values_as_unsupported() {
        let source = r#"
            #[derive(Dto)]
            struct Attachment {
                #[dto(int = "magic")]
                sequence: u128,
                #[dto(bytes = "raw")]
                payload: Vec<u8>,
            }
        "#;

        let inventory = scan_rust_source("src/sdk.rs", source).unwrap();
        let unsupported_attrs = inventory
            .findings
            .iter()
            .filter(|finding| finding.code == "INV0300")
            .map(|finding| finding.attribute.as_deref().unwrap_or_default())
            .collect::<Vec<_>>();

        assert!(unsupported_attrs.contains(&"dto::int"));
        assert!(unsupported_attrs.contains(&"dto::bytes"));
    }

    #[test]
    fn ignores_plain_helper_items_without_inventory_derives() {
        let source = r#"
            use serde::{Deserialize, Serialize};

            #[derive(Debug)]
            struct HelperOptions {
                output: std::path::PathBuf,
            }

            #[derive(Debug, thiserror::Error)]
            enum HelperError {
                #[error("bad input")]
                BadInput(String),
            }

            #[derive(Serialize, Deserialize)]
            struct PublicDto {
                helper: HelperOptions,
            }
        "#;

        let inventory = scan_rust_source("src/helpers.rs", source).unwrap();

        assert_eq!(
            inventory
                .items
                .iter()
                .map(|item| item.rust_name.as_str())
                .collect::<Vec<_>>(),
            vec!["PublicDto"]
        );
        assert!(!inventory.findings.iter().any(|finding| {
            finding.type_name.as_deref() == Some("HelperError") && finding.code == "INV1005"
        }));
        assert!(inventory.findings.iter().any(|finding| {
            finding.code == "INV1101"
                && finding.type_name.as_deref() == Some("PublicDto")
                && finding.field_name.as_deref() == Some("helper")
        }));
    }

    #[test]
    fn reports_custom_default_paths_as_unsupported() {
        let source = r#"
            #[derive(Dto)]
            struct Defaults {
                #[serde(default = "fallback")]
                tags: Vec<String>,
            }
        "#;

        let inventory = scan_rust_source("src/defaults.rs", source).unwrap();

        assert!(inventory.findings.iter().any(|finding| {
            finding.code == "INV0300" && finding.attribute.as_deref() == Some("serde::default")
        }));
    }

    #[test]
    fn builds_deterministic_inventory_reports_from_manifest() {
        let manifest = InventoryManifest::from_toml_str(
            r#"
            roots = ["UserProfile"]

            [sdk]
            root = "."
            package = "sdk"
            source_files = ["src/sdk.rs"]

            [typescript]
            generated_artifact_policy = "checked_in"

            [typescript.package_shape]
            package = "sdk"
            out_dir = "generated/ts"
            emit = "ts"

            [python]
            generated_artifact_policy = "build_time"

            [python.package_shape]
            package = "sdk_dto"
            out_dir = "generated/python/sdk_dto"
            "#,
        )
        .unwrap();
        let inventory = scan_rust_source(
            "src/sdk.rs",
            r#"
            #[derive(Serialize, Dto)]
            struct UserProfile {
                #[serde(skip)]
                internal_note: String,
                id: uuid::Uuid,
                balance: u128,
            }
            "#,
        )
        .unwrap();

        let report = build_inventory_report(manifest, vec![inventory]);
        let first_json = render_inventory_json(&report).unwrap();
        let second_json = render_inventory_json(&report).unwrap();

        assert_eq!(first_json, second_json);
        assert!(first_json.contains("\"schema_version\": 1"));
        assert_eq!(report.serde.skipped_fields.len(), 1);
        assert_eq!(report.types.third_party_fields.len(), 1);
        assert_eq!(report.types.large_integer_fields.len(), 1);
        assert!(report.promotions.required.iter().any(|decision| {
            decision.feature.contains("uuid") && decision.decision == "required_mapping_review"
        }));
        assert!(report.promotions.required.iter().any(|decision| {
            decision.feature.contains("large_integer_policy")
                && decision.decision == "required_numeric_policy"
        }));
    }

    #[test]
    fn renders_markdown_report_with_promotion_decisions() {
        let manifest = InventoryManifest::default();
        let inventory = scan_rust_source(
            "src/sdk.rs",
            r#"
            #[derive(Serialize, Dto)]
            struct UserProfile {
                #[serde(flatten)]
                metadata: serde_json::Value,
            }
            "#,
        )
        .unwrap();
        let report = build_inventory_report(manifest, vec![inventory]);
        let markdown = render_inventory_markdown(&report);

        assert!(markdown.contains("# SDK Inventory Pilot Report"));
        assert!(markdown.contains("## Promotion Decisions"));
        assert!(markdown.trim_end().lines().last().unwrap().starts_with('|'));
        assert!(markdown.contains("serde::flatten"));
    }
}
