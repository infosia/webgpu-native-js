#![warn(missing_docs)]
//! Parses the pinned WebIDL and C-API YAML inputs and joins their shared surface.
//!
//! This crate intentionally reports a typed intermediate model. It does not emit
//! code into `core`; emission belongs to the next codegen slice.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{self, Write as _};

use serde::Deserialize;
use weedle::argument::Argument;
use weedle::attribute::{ExtendedAttribute, ExtendedAttributeList};
use weedle::common::Default as IdlDefault;
use weedle::dictionary::DictionaryMember;
use weedle::interface::InterfaceMember;
use weedle::literal::{DefaultValue, FloatLit, IntegerLit};
use weedle::mixin::MixinMember;
use weedle::types::{
    AttributedType, FloatingPointType, IntegerType, MayBeNull, NonAnyType, ReturnType, Type,
    UnionMemberType,
};
use weedle::{Definition, Definitions, Parse};

/// A failure produced while parsing or joining codegen inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodegenError {
    /// WebIDL parsing failed or left unconsumed input.
    Idl(String),
    /// YAML deserialization failed.
    Yaml(String),
    /// Policy TOML deserialization or validation failed.
    Policy(String),
}

impl fmt::Display for CodegenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idl(message) => write!(formatter, "WebIDL error: {message}"),
            Self::Yaml(message) => write!(formatter, "YAML error: {message}"),
            Self::Policy(message) => write!(formatter, "policy error: {message}"),
        }
    }
}

impl std::error::Error for CodegenError {}

/// Counts of WebIDL definition forms consumed by weedle2.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConstructCounts {
    /// Ordinary and partial interface definitions.
    pub interfaces: usize,
    /// Interface mixin definitions.
    pub mixins: usize,
    /// `includes` statements.
    pub includes: usize,
    /// Dictionary definitions.
    pub dictionaries: usize,
    /// Enum definitions.
    pub enums: usize,
    /// Typedef definitions.
    pub typedefs: usize,
    /// Namespace definitions.
    pub namespaces: usize,
}

/// Evidence about the weedle2 full-file parse decision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParserEvidence {
    /// Number of parsed top-level definitions.
    pub definitions: usize,
    /// Bytes left after parsing; a successful complete parse has zero.
    pub remaining_bytes: usize,
    /// Definition-form counts.
    pub constructs: ConstructCounts,
    /// Exact namespace-constant declarations rewritten by the pre-pass.
    pub namespace_const_rewrites: Vec<String>,
    /// Whether `[EnforceRange]` occurred in the consumed input.
    pub saw_enforce_range: bool,
    /// Whether `[SameObject]` occurred in the consumed input.
    pub saw_same_object: bool,
    /// Whether `[Exposed=...]` occurred in the consumed input.
    pub saw_exposed: bool,
}

/// One typed value on either side of a joined member.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValueModel {
    /// Argument, result, or field name.
    pub name: String,
    /// Source-side type spelling.
    pub type_name: String,
    /// Source-side default spelling, when present.
    pub default_value: Option<String>,
    /// Whether WebIDL applies `[EnforceRange]` directly or through a typedef.
    pub enforce_range: bool,
    /// Whether WebIDL permits `null`.
    pub nullable: bool,
    /// Whether the value is required rather than optional/omittable.
    pub required: bool,
    /// C pointer qualification (`immutable` or `mutable`), when present.
    pub pointer: Option<String>,
    /// Whether a C array is represented as a count plus pointer.
    pub count_and_pointer: bool,
    /// Whether the C value is represented by `WGPUStringView`.
    pub string_view: bool,
    /// Whether the C struct participates in a chained-struct layout.
    pub chained: bool,
}

/// The kind of a WebIDL member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdlMemberKind {
    /// An operation.
    Operation,
    /// An attribute.
    Attribute,
    /// A dictionary field.
    DictionaryField,
}

/// WebIDL facts for one operation overload, attribute, or dictionary field.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IdlMemberModel {
    /// WebIDL member name.
    pub name: String,
    /// Member category.
    pub kind: IdlMemberKind,
    /// Return value followed by arguments, or the single attribute/field value.
    pub values: Vec<ValueModel>,
    /// Whether the member has `[SameObject]`.
    pub same_object: bool,
}

/// C-ABI facts for one function or struct member.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CMemberModel {
    /// Derived public C spelling such as `wgpuDeviceCreateBuffer`.
    pub name: String,
    /// Return value followed by arguments, or the single struct field value.
    pub values: Vec<ValueModel>,
    /// Callback-info type used by an asynchronous C function, when present.
    pub callback: Option<String>,
}

/// A selected member pairing in the typed join model.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemberPair {
    /// Owning WebIDL interface or dictionary.
    pub owner: String,
    /// Logical WebIDL member name.
    pub member: String,
    /// All WebIDL overloads for this logical member.
    pub idl: Vec<IdlMemberModel>,
    /// Matching C function or field.
    pub c: CMemberModel,
}

/// A joined interface, dictionary, enum, typedef, or C-only chained struct.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TypePair {
    /// WebIDL type name, if one exists.
    pub idl_name: Option<String>,
    /// Derived C type spelling, if one exists.
    pub c_name: Option<String>,
    /// Whether the C struct has an extensible or extension chain header.
    pub c_chained: bool,
    /// Joined selected members or dictionary fields.
    pub members: Vec<MemberPair>,
}

/// One loud difference between the WebIDL and C sides.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct Mismatch {
    /// Stable, human-readable mismatch text.
    pub message: String,
}

/// The complete parser and join report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct JoinReport {
    /// Parser-decision evidence.
    pub parser: ParserEvidence,
    /// Joined subset interfaces.
    pub interfaces: Vec<TypePair>,
    /// Transitively referenced dictionaries and C structs.
    pub dictionaries: Vec<TypePair>,
    /// Transitively referenced enums, bitflags, and typedefs.
    pub enums: Vec<TypePair>,
    /// Loud one-sided names.
    pub mismatches: Vec<Mismatch>,
    /// Joined interfaces or members omitted by the policy subset.
    pub skips: Vec<String>,
}

/// Parses pinned-format inputs, enforces the subset policy, and returns a typed join.
pub fn join_inputs(idl: &str, yaml: &str, policy: &str) -> Result<JoinReport, CodegenError> {
    let (cooked, rewrites) = preprocess_namespace_consts(idl)?;
    let (remaining, definitions) = Definitions::parse(&cooked)
        .map_err(|error| CodegenError::Idl(format!("weedle2: {error:?}")))?;
    if !remaining.is_empty() {
        return Err(CodegenError::Idl(format!(
            "weedle2 stopped before: {:?}",
            remaining.chars().take(240).collect::<String>()
        )));
    }

    let yaml: YamlRoot =
        serde_yaml::from_str(yaml).map_err(|error| CodegenError::Yaml(error.to_string()))?;
    let policy: Policy =
        toml::from_str(policy).map_err(|error| CodegenError::Policy(error.to_string()))?;
    if policy.schema_version != 1 {
        return Err(CodegenError::Policy(format!(
            "unsupported schema_version {}; expected 1",
            policy.schema_version
        )));
    }

    let index = IdlIndex::new(&definitions);
    validate_policy(&policy, &index)?;
    Ok(build_report(
        idl,
        definitions.len(),
        rewrites,
        &index,
        &yaml,
        &policy,
    ))
}

/// Renders a deterministic text report suitable for the `report` CLI and reviews.
#[must_use]
pub fn render_report(report: &JoinReport) -> String {
    let mut output = String::new();
    let parser = &report.parser;
    let _ = writeln!(
        output,
        "parser: weedle2 5.0.0 with namespace-const pre-pass"
    );
    let _ = writeln!(
        output,
        "definitions: {} (remaining bytes: {})",
        parser.definitions, parser.remaining_bytes
    );
    let _ = writeln!(
        output,
        "constructs: interfaces={} mixins={} includes={} dictionaries={} enums={} typedefs={} namespaces={}",
        parser.constructs.interfaces,
        parser.constructs.mixins,
        parser.constructs.includes,
        parser.constructs.dictionaries,
        parser.constructs.enums,
        parser.constructs.typedefs,
        parser.constructs.namespaces
    );
    let _ = writeln!(
        output,
        "extended attributes: EnforceRange={} SameObject={} Exposed={}",
        parser.saw_enforce_range, parser.saw_same_object, parser.saw_exposed
    );
    let _ = writeln!(
        output,
        "namespace const rewrites: {}",
        parser.namespace_const_rewrites.len()
    );
    for declaration in &parser.namespace_const_rewrites {
        let _ = writeln!(output, "  weedle2 unsupported exact text: {declaration}");
    }
    let member_count: usize = report
        .interfaces
        .iter()
        .chain(report.dictionaries.iter())
        .map(|pair| pair.members.len())
        .sum();
    let _ = writeln!(
        output,
        "join: interfaces={} dictionaries={} enums/typedefs={} member_pairs={}",
        report.interfaces.len(),
        report.dictionaries.len(),
        report.enums.len(),
        member_count
    );
    let _ = writeln!(output, "mismatches: {}", report.mismatches.len());
    for mismatch in &report.mismatches {
        let _ = writeln!(output, "  {}", mismatch.message);
    }
    let _ = writeln!(output, "skips: {}", report.skips.len());
    for skip in &report.skips {
        let _ = writeln!(output, "  {skip}");
    }
    output
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema_version: u32,
    subset: Vec<SubsetEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubsetEntry {
    interface: String,
    #[serde(default)]
    members: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct YamlRoot {
    #[serde(default)]
    enums: Vec<YamlEnum>,
    #[serde(default)]
    bitflags: Vec<YamlEnum>,
    #[serde(default)]
    structs: Vec<YamlStruct>,
    #[serde(default)]
    functions: Vec<YamlFunction>,
    #[serde(default)]
    objects: Vec<YamlObject>,
}

#[derive(Debug, Deserialize)]
struct YamlEnum {
    name: String,
    #[serde(default)]
    entries: Vec<Option<YamlEnumEntry>>,
}

#[derive(Debug, Deserialize)]
struct YamlEnumEntry {
    name: String,
}

#[derive(Debug, Deserialize)]
struct YamlStruct {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    extends: Vec<String>,
    #[serde(default)]
    members: Vec<YamlValue>,
}

#[derive(Debug, Deserialize)]
struct YamlObject {
    name: String,
    #[serde(default)]
    methods: Vec<YamlFunction>,
}

#[derive(Debug, Deserialize)]
struct YamlFunction {
    name: String,
    returns: Option<YamlValue>,
    #[serde(default)]
    args: Vec<YamlValue>,
    callback: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YamlValue {
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    type_name: Option<String>,
    pointer: Option<String>,
    #[serde(default)]
    optional: bool,
    default: Option<serde_yaml::Value>,
}

struct IdlIndex<'a> {
    interfaces: BTreeMap<String, Vec<&'a InterfaceMember<'a>>>,
    mixins: BTreeMap<String, Vec<&'a MixinMember<'a>>>,
    includes: BTreeMap<String, Vec<String>>,
    dictionaries: BTreeMap<String, Vec<&'a DictionaryMember<'a>>>,
    dictionary_parents: BTreeMap<String, String>,
    enums: BTreeMap<String, Vec<String>>,
    typedefs: BTreeMap<String, TypedefFacts>,
    constructs: ConstructCounts,
}

#[derive(Clone)]
struct TypedefFacts {
    type_name: String,
    enforce_range: bool,
}

impl<'a> IdlIndex<'a> {
    fn new(definitions: &'a Definitions<'a>) -> Self {
        let mut this = Self {
            interfaces: BTreeMap::new(),
            mixins: BTreeMap::new(),
            includes: BTreeMap::new(),
            dictionaries: BTreeMap::new(),
            dictionary_parents: BTreeMap::new(),
            enums: BTreeMap::new(),
            typedefs: BTreeMap::new(),
            constructs: ConstructCounts::default(),
        };
        for definition in definitions {
            match definition {
                Definition::Interface(value) => {
                    this.constructs.interfaces += 1;
                    this.interfaces
                        .entry(value.identifier.0.to_owned())
                        .or_default()
                        .extend(value.members.body.iter());
                }
                Definition::PartialInterface(value) => {
                    this.constructs.interfaces += 1;
                    this.interfaces
                        .entry(value.identifier.0.to_owned())
                        .or_default()
                        .extend(value.members.body.iter());
                }
                Definition::InterfaceMixin(value) => {
                    this.constructs.mixins += 1;
                    this.mixins
                        .entry(value.identifier.0.to_owned())
                        .or_default()
                        .extend(value.members.body.iter());
                }
                Definition::PartialInterfaceMixin(value) => {
                    this.constructs.mixins += 1;
                    this.mixins
                        .entry(value.identifier.0.to_owned())
                        .or_default()
                        .extend(value.members.body.iter());
                }
                Definition::IncludesStatement(value) => {
                    this.constructs.includes += 1;
                    this.includes
                        .entry(value.lhs_identifier.0.to_owned())
                        .or_default()
                        .push(value.rhs_identifier.0.to_owned());
                }
                Definition::Dictionary(value) => {
                    this.constructs.dictionaries += 1;
                    let name = value.identifier.0.to_owned();
                    this.dictionaries
                        .entry(name.clone())
                        .or_default()
                        .extend(value.members.body.iter());
                    if let Some(parent) = value.inheritance {
                        this.dictionary_parents
                            .insert(name, parent.identifier.0.to_owned());
                    }
                }
                Definition::PartialDictionary(value) => {
                    this.constructs.dictionaries += 1;
                    this.dictionaries
                        .entry(value.identifier.0.to_owned())
                        .or_default()
                        .extend(value.members.body.iter());
                }
                Definition::Enum(value) => {
                    this.constructs.enums += 1;
                    this.enums.insert(
                        value.identifier.0.to_owned(),
                        value
                            .values
                            .body
                            .list
                            .iter()
                            .map(|entry| entry.value.0.to_owned())
                            .collect(),
                    );
                }
                Definition::Typedef(value) => {
                    this.constructs.typedefs += 1;
                    this.typedefs.insert(
                        value.identifier.0.to_owned(),
                        TypedefFacts {
                            type_name: render_type(&value.type_.type_),
                            enforce_range: attrs_contain(
                                value.type_.attributes.as_ref(),
                                "EnforceRange",
                            ),
                        },
                    );
                }
                Definition::Namespace(_) | Definition::PartialNamespace(_) => {
                    this.constructs.namespaces += 1;
                }
                Definition::Callback(_)
                | Definition::CallbackInterface(_)
                | Definition::Implements(_) => {}
            }
        }
        this
    }

    fn effective_members(&self, interface: &str) -> Vec<IdlMemberModel> {
        let mut members = Vec::new();
        if let Some(own) = self.interfaces.get(interface) {
            members.extend(
                own.iter()
                    .filter_map(|member| self.interface_member(member)),
            );
        }
        if let Some(includes) = self.includes.get(interface) {
            for mixin in includes {
                if let Some(mixin_members) = self.mixins.get(mixin) {
                    members.extend(
                        mixin_members
                            .iter()
                            .filter_map(|member| self.mixin_member(member)),
                    );
                }
            }
        }
        members
    }

    fn interface_member(&self, member: &InterfaceMember<'a>) -> Option<IdlMemberModel> {
        match member {
            InterfaceMember::Operation(value) => Some(IdlMemberModel {
                name: value.identifier?.0.to_owned(),
                kind: IdlMemberKind::Operation,
                values: operation_values(&value.return_type, &value.args.body.list, self),
                same_object: attrs_contain(value.attributes.as_ref(), "SameObject"),
            }),
            InterfaceMember::Attribute(value) => Some(IdlMemberModel {
                name: value.identifier.0.to_owned(),
                kind: IdlMemberKind::Attribute,
                values: vec![idl_value("value", &value.type_, None, true, self)],
                same_object: attrs_contain(value.attributes.as_ref(), "SameObject"),
            }),
            _ => None,
        }
    }

    fn mixin_member(&self, member: &MixinMember<'a>) -> Option<IdlMemberModel> {
        match member {
            MixinMember::Operation(value) => Some(IdlMemberModel {
                name: value.identifier?.0.to_owned(),
                kind: IdlMemberKind::Operation,
                values: operation_values(&value.return_type, &value.args.body.list, self),
                same_object: attrs_contain(value.attributes.as_ref(), "SameObject"),
            }),
            MixinMember::Attribute(value) => Some(IdlMemberModel {
                name: value.identifier.0.to_owned(),
                kind: IdlMemberKind::Attribute,
                values: vec![idl_value("value", &value.type_, None, true, self)],
                same_object: attrs_contain(value.attributes.as_ref(), "SameObject"),
            }),
            _ => None,
        }
    }

    fn dictionary_members(&self, dictionary: &str) -> Vec<IdlMemberModel> {
        let mut output = Vec::new();
        if let Some(parent) = self.dictionary_parents.get(dictionary) {
            output.extend(self.dictionary_members(parent));
        }
        if let Some(members) = self.dictionaries.get(dictionary) {
            output.extend(members.iter().map(|member| IdlMemberModel {
                name: member.identifier.0.to_owned(),
                kind: IdlMemberKind::DictionaryField,
                values: vec![idl_plain_value(
                    member.identifier.0,
                    &member.type_,
                    member.default.as_ref(),
                    member.required.is_some(),
                    member.attributes.as_ref(),
                    self,
                )],
                same_object: attrs_contain(member.attributes.as_ref(), "SameObject"),
            }));
        }
        output
    }
}

fn preprocess_namespace_consts(idl: &str) -> Result<(String, Vec<String>), CodegenError> {
    let mut cooked = String::with_capacity(idl.len());
    let mut rewrites = Vec::new();
    let mut in_namespace = false;
    for line in idl.lines() {
        if line.contains("namespace ") {
            in_namespace = true;
        }
        let trimmed = line.trim();
        if in_namespace && trimmed.starts_with("const ") {
            let declaration = trimmed.trim_start_matches("const ");
            let (left, _) = declaration.split_once('=').ok_or_else(|| {
                CodegenError::Idl(format!("malformed namespace constant: {trimmed}"))
            })?;
            rewrites.push(trimmed.to_owned());
            cooked.push_str("    readonly attribute ");
            cooked.push_str(left.trim());
            cooked.push_str(";\n");
        } else {
            cooked.push_str(line);
            cooked.push('\n');
        }
        if in_namespace && trimmed == "};" {
            in_namespace = false;
        }
    }
    Ok((cooked, rewrites))
}

fn validate_policy(policy: &Policy, index: &IdlIndex<'_>) -> Result<(), CodegenError> {
    let mut seen = BTreeSet::new();
    for entry in &policy.subset {
        if !seen.insert(entry.interface.as_str()) {
            return Err(CodegenError::Policy(format!(
                "duplicate subset interface {}",
                entry.interface
            )));
        }
        if !index.interfaces.contains_key(&entry.interface) {
            return Err(CodegenError::Policy(format!(
                "unknown subset interface {}",
                entry.interface
            )));
        }
        let available: BTreeSet<String> = index
            .effective_members(&entry.interface)
            .into_iter()
            .map(|member| member.name)
            .collect();
        for member in &entry.members {
            if !available.contains(member) {
                return Err(CodegenError::Policy(format!(
                    "unknown subset member {}.{member}",
                    entry.interface
                )));
            }
        }
    }
    Ok(())
}

fn build_report(
    raw_idl: &str,
    definition_count: usize,
    rewrites: Vec<String>,
    index: &IdlIndex<'_>,
    yaml: &YamlRoot,
    policy: &Policy,
) -> JoinReport {
    // Top-level YAML functions are parsed as part of the deliberately small C
    // model; this slice's interface subset joins object methods only.
    let _top_level_function_count = yaml.functions.len();
    let object_map: BTreeMap<String, &YamlObject> = yaml
        .objects
        .iter()
        .map(|object| (canonical(&object.name), object))
        .collect();
    let selected: BTreeSet<&str> = policy
        .subset
        .iter()
        .map(|entry| entry.interface.as_str())
        .collect();
    let mut report = JoinReport {
        parser: ParserEvidence {
            definitions: definition_count,
            remaining_bytes: 0,
            constructs: index.constructs.clone(),
            namespace_const_rewrites: rewrites,
            saw_enforce_range: raw_idl.contains("[EnforceRange]"),
            saw_same_object: raw_idl.contains("SameObject"),
            saw_exposed: raw_idl.contains("Exposed="),
        },
        ..JoinReport::default()
    };

    for (name, members) in &index.interfaces {
        let key = canonical(idl_base_name(name));
        if object_map.contains_key(&key) && !selected.contains(name.as_str()) {
            report
                .skips
                .push(format!("interface {name} (outside policy subset)"));
        }
        let _ = members;
    }

    let mut idl_type_roots = BTreeSet::new();
    let mut c_type_roots = BTreeSet::new();
    for entry in &policy.subset {
        let effective = index.effective_members(&entry.interface);
        let object = object_map
            .get(&canonical(idl_base_name(&entry.interface)))
            .copied();
        let mut pair = TypePair {
            idl_name: Some(entry.interface.clone()),
            c_name: object.map(|value| c_type_name(&value.name)),
            c_chained: false,
            members: Vec::new(),
        };
        let Some(object) = object else {
            report.mismatches.push(mismatch(format!(
                "interface {}: IDL-only type (expected YAML object {})",
                entry.interface,
                snake_case(idl_base_name(&entry.interface))
            )));
            report.interfaces.push(pair);
            continue;
        };
        let c_by_name: BTreeMap<String, &YamlFunction> = object
            .methods
            .iter()
            .map(|function| (canonical(&function.name), function))
            .collect();
        let mut matched_c = BTreeSet::new();

        let mut grouped: BTreeMap<String, Vec<IdlMemberModel>> = BTreeMap::new();
        for member in effective {
            grouped.entry(member.name.clone()).or_default().push(member);
        }
        for (member_name, overloads) in &grouped {
            let candidates = c_member_candidates(&overloads[0]);
            let found = candidates
                .iter()
                .find_map(|candidate| c_by_name.get(candidate).copied());
            if let Some(function) = found {
                matched_c.insert(canonical(&function.name));
                if entry.members.iter().any(|selected| selected == member_name) {
                    for overload in overloads {
                        collect_idl_roots(&overload.values, &mut idl_type_roots);
                    }
                    collect_c_function_roots(function, &mut c_type_roots);
                    pair.members.push(MemberPair {
                        owner: entry.interface.clone(),
                        member: member_name.clone(),
                        idl: overloads.clone(),
                        c: c_function_model(&object.name, function, yaml),
                    });
                } else {
                    report.skips.push(format!(
                        "member {}.{member_name} (outside policy subset)",
                        entry.interface
                    ));
                }
            } else {
                report.mismatches.push(mismatch(format!(
                    "interface {}: IDL-only member {member_name}",
                    entry.interface
                )));
            }
        }
        for function in &object.methods {
            if !matched_c.contains(&canonical(&function.name)) {
                report.mismatches.push(mismatch(format!(
                    "interface {}: C-only member {}",
                    entry.interface,
                    c_function_name(&object.name, &function.name)
                )));
            }
        }
        report.interfaces.push(pair);
    }

    build_transitive_types(index, yaml, idl_type_roots, c_type_roots, &mut report);
    report.mismatches.sort();
    report.mismatches.dedup();
    report.skips.sort();
    report.skips.dedup();
    report
}

fn build_transitive_types(
    index: &IdlIndex<'_>,
    yaml: &YamlRoot,
    idl_roots: BTreeSet<String>,
    c_roots: BTreeSet<String>,
    report: &mut JoinReport,
) {
    let idl_types = transitive_idl_types(index, idl_roots);
    let c_types = transitive_c_types(yaml, c_roots);
    let struct_map: BTreeMap<String, &YamlStruct> = yaml
        .structs
        .iter()
        .map(|value| (canonical(&value.name), value))
        .collect();
    let enum_map: BTreeMap<String, (&YamlEnum, bool)> = yaml
        .enums
        .iter()
        .map(|value| (canonical(&value.name), (value, false)))
        .chain(
            yaml.bitflags
                .iter()
                .map(|value| (canonical(&value.name), (value, true))),
        )
        .collect();
    let mut matched_c_structs = BTreeSet::new();
    let mut matched_c_enums = BTreeSet::new();

    for name in &idl_types {
        if index.dictionaries.contains_key(name) {
            let candidate = canonical(idl_base_name(name));
            let c_struct = struct_map.get(&candidate).copied();
            let mut pair = TypePair {
                idl_name: Some(name.clone()),
                c_name: c_struct.map(|value| c_type_name(&value.name)),
                c_chained: c_struct.is_some_and(is_chained_struct),
                members: Vec::new(),
            };
            if let Some(c_struct) = c_struct {
                matched_c_structs.insert(c_struct.name.clone());
                join_dictionary_fields(index, name, c_struct, yaml, &mut pair, report);
            } else {
                report
                    .mismatches
                    .push(mismatch(format!("dictionary {name}: IDL-only type")));
            }
            report.dictionaries.push(pair);
        } else if let Some(values) = index.enums.get(name) {
            let candidate = canonical(idl_base_name(name));
            let c_enum = enum_map.get(&candidate).copied();
            let pair = TypePair {
                idl_name: Some(name.clone()),
                c_name: c_enum.map(|(value, _)| c_type_name(&value.name)),
                c_chained: false,
                members: Vec::new(),
            };
            if let Some((c_enum, _)) = c_enum {
                matched_c_enums.insert(c_enum.name.clone());
                let idl_entries: BTreeSet<String> =
                    values.iter().map(|value| canonical(value)).collect();
                let c_entries: BTreeSet<String> = c_enum
                    .entries
                    .iter()
                    .filter_map(Option::as_ref)
                    .map(|value| canonical(&value.name))
                    .collect();
                for value in values {
                    if !c_entries.contains(&canonical(value)) {
                        report
                            .mismatches
                            .push(mismatch(format!("enum {name}: IDL-only value {value}")));
                    }
                }
                for entry in c_enum.entries.iter().filter_map(Option::as_ref) {
                    if !idl_entries.contains(&canonical(&entry.name)) {
                        report.mismatches.push(mismatch(format!(
                            "enum {name}: C-only value {}",
                            entry.name
                        )));
                    }
                }
            } else {
                report
                    .mismatches
                    .push(mismatch(format!("enum {name}: IDL-only type")));
            }
            report.enums.push(pair);
        } else if let Some(typedef) = index.typedefs.get(name) {
            let candidates = typedef_c_candidates(name);
            let c_enum = candidates
                .iter()
                .find_map(|candidate| enum_map.get(candidate).copied());
            if let Some((value, _)) = c_enum {
                matched_c_enums.insert(value.name.clone());
            }
            report.enums.push(TypePair {
                idl_name: Some(format!("{name} = {}", typedef.type_name)),
                c_name: c_enum.map(|(value, _)| c_type_name(&value.name)),
                c_chained: false,
                members: Vec::new(),
            });
        }
    }

    for c_type in c_types {
        if let Some(name) = c_type.strip_prefix("struct.") {
            if !matched_c_structs.contains(name) {
                if let Some(value) = yaml.structs.iter().find(|value| value.name == name) {
                    report.mismatches.push(mismatch(format!(
                        "transitive C-only struct {}",
                        c_type_name(&value.name)
                    )));
                    report.dictionaries.push(TypePair {
                        idl_name: None,
                        c_name: Some(c_type_name(&value.name)),
                        c_chained: is_chained_struct(value),
                        members: Vec::new(),
                    });
                }
            }
        } else if let Some(name) = c_type
            .strip_prefix("enum.")
            .or_else(|| c_type.strip_prefix("bitflag."))
        {
            if !matched_c_enums.contains(name) {
                if let Some((value, _)) = yaml
                    .enums
                    .iter()
                    .map(|value| (value, false))
                    .chain(yaml.bitflags.iter().map(|value| (value, true)))
                    .find(|(value, _)| value.name == name)
                {
                    report.mismatches.push(mismatch(format!(
                        "transitive C-only enum/bitflag {}",
                        c_type_name(&value.name)
                    )));
                    report.enums.push(TypePair {
                        idl_name: None,
                        c_name: Some(c_type_name(&value.name)),
                        c_chained: false,
                        members: Vec::new(),
                    });
                }
            }
        }
    }
    report.dictionaries.sort_by(|left, right| {
        left.idl_name
            .cmp(&right.idl_name)
            .then(left.c_name.cmp(&right.c_name))
    });
    report.enums.sort_by(|left, right| {
        left.idl_name
            .cmp(&right.idl_name)
            .then(left.c_name.cmp(&right.c_name))
    });
}

fn join_dictionary_fields(
    index: &IdlIndex<'_>,
    dictionary: &str,
    c_struct: &YamlStruct,
    yaml: &YamlRoot,
    pair: &mut TypePair,
    report: &mut JoinReport,
) {
    let idl_members = index.dictionary_members(dictionary);
    let c_members: BTreeMap<String, &YamlValue> = c_struct
        .members
        .iter()
        .map(|value| (canonical(&value.name), value))
        .collect();
    let mut matched = BTreeSet::new();
    for member in idl_members {
        let key = canonical(&member.name);
        if let Some(c_member) = c_members.get(&key).copied() {
            matched.insert(key);
            pair.members.push(MemberPair {
                owner: dictionary.to_owned(),
                member: member.name.clone(),
                idl: vec![member],
                c: CMemberModel {
                    name: c_member.name.clone(),
                    values: vec![c_value(c_member, yaml)],
                    callback: None,
                },
            });
        } else {
            report.mismatches.push(mismatch(format!(
                "dictionary {dictionary}: IDL-only member {}",
                member.name
            )));
        }
    }
    for member in &c_struct.members {
        if !matched.contains(&canonical(&member.name)) {
            report.mismatches.push(mismatch(format!(
                "dictionary {dictionary}: C-only member {}",
                member.name
            )));
        }
    }
}

fn transitive_idl_types(index: &IdlIndex<'_>, roots: BTreeSet<String>) -> BTreeSet<String> {
    let mut pending: VecDeque<String> = roots.into_iter().collect();
    let mut found = BTreeSet::new();
    while let Some(name) = pending.pop_front() {
        if !name.starts_with("GPU") || !found.insert(name.clone()) {
            continue;
        }
        if index.dictionaries.contains_key(&name) {
            if let Some(parent) = index.dictionary_parents.get(&name) {
                pending.push_back(parent.clone());
            }
            for member in index.dictionary_members(&name) {
                for value in member.values {
                    pending.extend(type_identifiers(&value.type_name));
                }
            }
        } else if let Some(typedef) = index.typedefs.get(&name) {
            pending.extend(type_identifiers(&typedef.type_name));
        }
    }
    found
}

fn transitive_c_types(yaml: &YamlRoot, roots: BTreeSet<String>) -> BTreeSet<String> {
    let mut pending: VecDeque<String> = roots.into_iter().collect();
    let mut found = BTreeSet::new();
    while let Some(type_name) = pending.pop_front() {
        for reference in c_type_references(&type_name) {
            if !found.insert(reference.clone()) {
                continue;
            }
            if let Some(struct_name) = reference.strip_prefix("struct.") {
                if let Some(value) = yaml.structs.iter().find(|value| value.name == struct_name) {
                    for member in &value.members {
                        if let Some(type_name) = &member.type_name {
                            pending.push_back(type_name.clone());
                        }
                    }
                    for extension in yaml.structs.iter().filter(|extension| {
                        extension.extends.iter().any(|base| base == struct_name)
                    }) {
                        pending.push_back(format!("struct.{}", extension.name));
                    }
                }
            }
        }
    }
    found
}

fn operation_values(
    return_type: &ReturnType<'_>,
    arguments: &[Argument<'_>],
    index: &IdlIndex<'_>,
) -> Vec<ValueModel> {
    let mut values = vec![idl_return_value(return_type, index)];
    values.extend(arguments.iter().map(|argument| match argument {
        Argument::Single(value) => idl_value(
            value.identifier.0,
            &value.type_,
            value.default.as_ref(),
            value.optional.is_none(),
            index,
        ),
        Argument::Variadic(value) => idl_plain_value(
            value.identifier.0,
            &value.type_,
            None,
            false,
            value.attributes.as_ref(),
            index,
        ),
    }));
    values
}

fn idl_return_value(return_type: &ReturnType<'_>, index: &IdlIndex<'_>) -> ValueModel {
    match return_type {
        ReturnType::Undefined(_) => ValueModel {
            name: "result".to_owned(),
            type_name: "undefined".to_owned(),
            required: true,
            ..ValueModel::default()
        },
        ReturnType::Type(value) => idl_plain_value("result", value, None, true, None, index),
    }
}

fn idl_value(
    name: &str,
    attributed: &AttributedType<'_>,
    default: Option<&IdlDefault<'_>>,
    required: bool,
    index: &IdlIndex<'_>,
) -> ValueModel {
    idl_plain_value(
        name,
        &attributed.type_,
        default,
        required,
        attributed.attributes.as_ref(),
        index,
    )
}

fn idl_plain_value(
    name: &str,
    type_: &Type<'_>,
    default: Option<&IdlDefault<'_>>,
    required: bool,
    attributes: Option<&ExtendedAttributeList<'_>>,
    index: &IdlIndex<'_>,
) -> ValueModel {
    let type_name = render_type(type_);
    let typedef_enforced = type_identifiers(&type_name).iter().any(|identifier| {
        index
            .typedefs
            .get(identifier)
            .is_some_and(|facts| facts.enforce_range)
    });
    ValueModel {
        name: name.to_owned(),
        type_name,
        default_value: default.map(render_default),
        enforce_range: attrs_contain(attributes, "EnforceRange") || typedef_enforced,
        nullable: type_is_nullable(type_),
        required,
        ..ValueModel::default()
    }
}

fn c_function_model(object: &str, function: &YamlFunction, yaml: &YamlRoot) -> CMemberModel {
    let mut values = Vec::new();
    if let Some(returns) = &function.returns {
        let mut result = c_value(returns, yaml);
        result.name = "result".to_owned();
        values.push(result);
    } else {
        values.push(ValueModel {
            name: "result".to_owned(),
            type_name: "void".to_owned(),
            required: true,
            ..ValueModel::default()
        });
    }
    values.extend(function.args.iter().map(|value| c_value(value, yaml)));
    CMemberModel {
        name: c_function_name(object, &function.name),
        values,
        callback: function.callback.as_deref().map(c_callback_name),
    }
}

fn c_value(value: &YamlValue, yaml: &YamlRoot) -> ValueModel {
    let source_type = value.type_name.as_deref().unwrap_or("void");
    let base_type = source_type
        .strip_prefix("array<")
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(source_type);
    let struct_name = base_type.strip_prefix("struct.");
    ValueModel {
        name: value.name.clone(),
        type_name: c_render_type(base_type),
        default_value: value.default.as_ref().map(yaml_scalar),
        enforce_range: false,
        nullable: value.optional,
        required: !value.optional,
        pointer: value.pointer.clone(),
        count_and_pointer: source_type.starts_with("array<"),
        string_view: source_type.starts_with("string") || source_type == "out_string",
        chained: struct_name.is_some_and(|name| {
            yaml.structs
                .iter()
                .find(|value| value.name == name)
                .is_some_and(is_chained_struct)
        }),
    }
}

fn c_member_candidates(member: &IdlMemberModel) -> Vec<String> {
    let base = canonical(&member.name);
    match member.kind {
        IdlMemberKind::Operation => vec![base],
        IdlMemberKind::Attribute if member.name == "label" => vec![canonical("set_label")],
        IdlMemberKind::Attribute => vec![canonical(&format!("get_{}", snake_case(&member.name)))],
        IdlMemberKind::DictionaryField => vec![base],
    }
}

fn collect_idl_roots(values: &[ValueModel], roots: &mut BTreeSet<String>) {
    for value in values {
        roots.extend(type_identifiers(&value.type_name));
    }
}

fn collect_c_function_roots(function: &YamlFunction, roots: &mut BTreeSet<String>) {
    if let Some(returns) = &function.returns {
        if let Some(type_name) = &returns.type_name {
            roots.insert(type_name.clone());
        }
    }
    for argument in &function.args {
        if let Some(type_name) = &argument.type_name {
            roots.insert(type_name.clone());
        }
    }
}

fn type_identifiers(type_name: &str) -> Vec<String> {
    type_name
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| token.starts_with("GPU"))
        .map(str::to_owned)
        .collect()
}

fn c_type_references(type_name: &str) -> Vec<String> {
    let mut output = Vec::new();
    for prefix in ["struct.", "enum.", "bitflag.", "object."] {
        let mut rest = type_name;
        while let Some(offset) = rest.find(prefix) {
            let start = offset;
            let tail = &rest[start..];
            let end = tail.find(['>', ',', ' ']).unwrap_or(tail.len());
            output.push(tail[..end].to_owned());
            rest = &tail[end..];
        }
    }
    output
}

fn typedef_c_candidates(name: &str) -> Vec<String> {
    let base = snake_case(idl_base_name(name));
    let mut candidates = vec![canonical(&base)];
    for suffix in ["_flags", "_usage_flags"] {
        if let Some(stripped) = base.strip_suffix(suffix) {
            let candidate = if suffix == "_usage_flags" {
                format!("{stripped}_usage")
            } else {
                stripped.to_owned()
            };
            candidates.push(canonical(&candidate));
        }
    }
    candidates
}

fn attrs_contain(attributes: Option<&ExtendedAttributeList<'_>>, wanted: &str) -> bool {
    attributes.is_some_and(|attributes| {
        attributes
            .body
            .list
            .iter()
            .any(|attribute| match attribute {
                ExtendedAttribute::NoArgs(value) => value.0 .0 == wanted,
                ExtendedAttribute::ArgList(value) => value.identifier.0 == wanted,
                ExtendedAttribute::NamedArgList(value) => value.lhs_identifier.0 == wanted,
                ExtendedAttribute::IdentList(value) => value.identifier.0 == wanted,
                ExtendedAttribute::Ident(value) => value.lhs_identifier.0 == wanted,
            })
    })
}

fn type_is_nullable(type_: &Type<'_>) -> bool {
    match type_ {
        Type::Single(value) => match value {
            weedle::types::SingleType::Any(_) => false,
            weedle::types::SingleType::NonAny(value) => non_any_nullable(value),
        },
        Type::Union(value) => value.q_mark.is_some(),
    }
}

fn non_any_nullable(type_: &NonAnyType<'_>) -> bool {
    match type_ {
        NonAnyType::Promise(_) => false,
        NonAnyType::Integer(value) => value.q_mark.is_some(),
        NonAnyType::FloatingPoint(value) => value.q_mark.is_some(),
        NonAnyType::Boolean(value) => value.q_mark.is_some(),
        NonAnyType::Byte(value) => value.q_mark.is_some(),
        NonAnyType::Octet(value) => value.q_mark.is_some(),
        NonAnyType::ByteString(value) => value.q_mark.is_some(),
        NonAnyType::DOMString(value) => value.q_mark.is_some(),
        NonAnyType::USVString(value) => value.q_mark.is_some(),
        NonAnyType::Sequence(value) => value.q_mark.is_some(),
        NonAnyType::Object(value) => value.q_mark.is_some(),
        NonAnyType::Symbol(value) => value.q_mark.is_some(),
        NonAnyType::Error(value) => value.q_mark.is_some(),
        NonAnyType::ArrayBuffer(value) => value.q_mark.is_some(),
        NonAnyType::DataView(value) => value.q_mark.is_some(),
        NonAnyType::Int8Array(value) => value.q_mark.is_some(),
        NonAnyType::Int16Array(value) => value.q_mark.is_some(),
        NonAnyType::Int32Array(value) => value.q_mark.is_some(),
        NonAnyType::Uint8Array(value) => value.q_mark.is_some(),
        NonAnyType::Uint16Array(value) => value.q_mark.is_some(),
        NonAnyType::Uint32Array(value) => value.q_mark.is_some(),
        NonAnyType::Uint8ClampedArray(value) => value.q_mark.is_some(),
        NonAnyType::Float32Array(value) => value.q_mark.is_some(),
        NonAnyType::Float64Array(value) => value.q_mark.is_some(),
        NonAnyType::ArrayBufferView(value) => value.q_mark.is_some(),
        NonAnyType::BufferSource(value) => value.q_mark.is_some(),
        NonAnyType::FrozenArrayType(value) => value.q_mark.is_some(),
        NonAnyType::RecordType(value) => value.q_mark.is_some(),
        NonAnyType::Identifier(value) => value.q_mark.is_some(),
    }
}

fn render_type(type_: &Type<'_>) -> String {
    match type_ {
        Type::Single(value) => match value {
            weedle::types::SingleType::Any(_) => "any".to_owned(),
            weedle::types::SingleType::NonAny(value) => render_non_any(value),
        },
        Type::Union(value) => nullable_suffix(
            format!(
                "({})",
                value
                    .type_
                    .body
                    .list
                    .iter()
                    .map(render_union_member)
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            value,
        ),
    }
}

fn render_union_member(value: &UnionMemberType<'_>) -> String {
    match value {
        UnionMemberType::Single(value) => render_non_any(&value.type_),
        UnionMemberType::Union(value) => nullable_suffix(
            format!(
                "({})",
                value
                    .type_
                    .body
                    .list
                    .iter()
                    .map(render_union_member)
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            value,
        ),
    }
}

fn render_non_any(type_: &NonAnyType<'_>) -> String {
    match type_ {
        NonAnyType::Promise(value) => format!(
            "Promise<{}>",
            match value.generics.body.as_ref() {
                ReturnType::Undefined(_) => "undefined".to_owned(),
                ReturnType::Type(value) => render_type(value),
            }
        ),
        NonAnyType::Integer(value) => nullable_suffix(render_integer(&value.type_), value),
        NonAnyType::FloatingPoint(value) => nullable_suffix(render_float(&value.type_), value),
        NonAnyType::Boolean(value) => nullable_suffix("boolean".to_owned(), value),
        NonAnyType::Byte(value) => nullable_suffix("byte".to_owned(), value),
        NonAnyType::Octet(value) => nullable_suffix("octet".to_owned(), value),
        NonAnyType::ByteString(value) => nullable_suffix("ByteString".to_owned(), value),
        NonAnyType::DOMString(value) => nullable_suffix("DOMString".to_owned(), value),
        NonAnyType::USVString(value) => nullable_suffix("USVString".to_owned(), value),
        NonAnyType::Sequence(value) => nullable_suffix(
            format!("sequence<{}>", render_type(&value.type_.generics.body)),
            value,
        ),
        NonAnyType::Object(value) => nullable_suffix("object".to_owned(), value),
        NonAnyType::Symbol(value) => nullable_suffix("symbol".to_owned(), value),
        NonAnyType::Error(value) => nullable_suffix("Error".to_owned(), value),
        NonAnyType::ArrayBuffer(value) => nullable_suffix("ArrayBuffer".to_owned(), value),
        NonAnyType::DataView(value) => nullable_suffix("DataView".to_owned(), value),
        NonAnyType::Int8Array(value) => nullable_suffix("Int8Array".to_owned(), value),
        NonAnyType::Int16Array(value) => nullable_suffix("Int16Array".to_owned(), value),
        NonAnyType::Int32Array(value) => nullable_suffix("Int32Array".to_owned(), value),
        NonAnyType::Uint8Array(value) => nullable_suffix("Uint8Array".to_owned(), value),
        NonAnyType::Uint16Array(value) => nullable_suffix("Uint16Array".to_owned(), value),
        NonAnyType::Uint32Array(value) => nullable_suffix("Uint32Array".to_owned(), value),
        NonAnyType::Uint8ClampedArray(value) => {
            nullable_suffix("Uint8ClampedArray".to_owned(), value)
        }
        NonAnyType::Float32Array(value) => nullable_suffix("Float32Array".to_owned(), value),
        NonAnyType::Float64Array(value) => nullable_suffix("Float64Array".to_owned(), value),
        NonAnyType::ArrayBufferView(value) => nullable_suffix("ArrayBufferView".to_owned(), value),
        NonAnyType::BufferSource(value) => nullable_suffix("BufferSource".to_owned(), value),
        NonAnyType::FrozenArrayType(value) => nullable_suffix(
            format!("FrozenArray<{}>", render_type(&value.type_.generics.body)),
            value,
        ),
        NonAnyType::RecordType(value) => nullable_suffix(
            format!("record<…, {}>", render_type(&value.type_.generics.body.2)),
            value,
        ),
        NonAnyType::Identifier(value) => nullable_suffix(value.type_.0.to_owned(), value),
    }
}

fn nullable_suffix<T>(mut rendered: String, value: &MayBeNull<T>) -> String {
    if value.q_mark.is_some() {
        rendered.push('?');
    }
    rendered
}

fn render_integer(value: &IntegerType) -> String {
    match value {
        IntegerType::LongLong(value) => format!(
            "{}long long",
            if value.unsigned.is_some() {
                "unsigned "
            } else {
                ""
            }
        ),
        IntegerType::Long(value) => format!(
            "{}long",
            if value.unsigned.is_some() {
                "unsigned "
            } else {
                ""
            }
        ),
        IntegerType::Short(value) => format!(
            "{}short",
            if value.unsigned.is_some() {
                "unsigned "
            } else {
                ""
            }
        ),
    }
}

fn render_float(value: &FloatingPointType) -> String {
    match value {
        FloatingPointType::Float(value) => format!(
            "{}float",
            if value.unrestricted.is_some() {
                "unrestricted "
            } else {
                ""
            }
        ),
        FloatingPointType::Double(value) => format!(
            "{}double",
            if value.unrestricted.is_some() {
                "unrestricted "
            } else {
                ""
            }
        ),
    }
}

fn render_default(value: &IdlDefault<'_>) -> String {
    match value.value {
        DefaultValue::Boolean(value) => value.0.to_string(),
        DefaultValue::EmptyArray(_) => "[]".to_owned(),
        DefaultValue::EmptyDictionary(_) => "{}".to_owned(),
        DefaultValue::Float(value) => match value {
            FloatLit::Value(value) => value.0.to_owned(),
            FloatLit::NegInfinity(_) => "-Infinity".to_owned(),
            FloatLit::Infinity(_) => "Infinity".to_owned(),
            FloatLit::NaN(_) => "NaN".to_owned(),
        },
        DefaultValue::Integer(value) => match value {
            IntegerLit::Dec(value) => value.0.to_owned(),
            IntegerLit::Hex(value) => value.0.to_owned(),
            IntegerLit::Oct(value) => value.0.to_owned(),
        },
        DefaultValue::Null(_) => "null".to_owned(),
        DefaultValue::String(value) => format!("\"{}\"", value.0),
    }
}

fn yaml_scalar(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::Null => "null".to_owned(),
        serde_yaml::Value::Bool(value) => value.to_string(),
        serde_yaml::Value::Number(value) => value.to_string(),
        serde_yaml::Value::String(value) => value.clone(),
        _ => format!("{value:?}"),
    }
}

fn is_chained_struct(value: &YamlStruct) -> bool {
    value.kind == "extensible" || value.kind == "extension"
}

fn idl_base_name(value: &str) -> &str {
    value.strip_prefix("GPU").unwrap_or(value)
}

fn canonical(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn snake_case(value: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = value.chars().collect();
    for (index, character) in chars.iter().copied().enumerate() {
        if character == '-' || character == ' ' {
            output.push('_');
            continue;
        }
        if character.is_ascii_uppercase() {
            let previous_lower = index > 0 && chars[index - 1].is_ascii_lowercase();
            let next_lower = chars.get(index + 1).is_some_and(char::is_ascii_lowercase);
            if index > 0 && !output.ends_with('_') && (previous_lower || next_lower) {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn pascal_case(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn c_type_name(yaml_name: &str) -> String {
    format!("WGPU{}", pascal_case(yaml_name))
}

fn c_function_name(object: &str, method: &str) -> String {
    format!("wgpu{}{}", pascal_case(object), pascal_case(method))
}

fn c_callback_name(callback: &str) -> String {
    let name = callback.strip_prefix("callback.").unwrap_or(callback);
    format!("WGPU{}CallbackInfo", pascal_case(name))
}

fn c_render_type(type_name: &str) -> String {
    if let Some(name) = type_name
        .strip_prefix("object.")
        .or_else(|| type_name.strip_prefix("struct."))
        .or_else(|| type_name.strip_prefix("enum."))
        .or_else(|| type_name.strip_prefix("bitflag."))
    {
        return c_type_name(name);
    }
    match type_name {
        "uint16" => "uint16_t".to_owned(),
        "uint32" => "uint32_t".to_owned(),
        "uint64" => "uint64_t".to_owned(),
        "int16" => "int16_t".to_owned(),
        "int32" => "int32_t".to_owned(),
        "int64" => "int64_t".to_owned(),
        "usize" => "size_t".to_owned(),
        "bool" => "WGPUBool".to_owned(),
        "float32" => "float".to_owned(),
        "float64" => "double".to_owned(),
        "string" | "string_with_default_empty" | "nullable_string" | "out_string" => {
            "WGPUStringView".to_owned()
        }
        "c_void" => "void".to_owned(),
        other => other.to_owned(),
    }
}

fn mismatch(message: String) -> Mismatch {
    Mismatch { message }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDL: &str = r#"
        interface GPUDevice { [SameObject] readonly attribute GPUQueue queue; GPUBuffer createBuffer(GPUBufferDescriptor descriptor); };
        dictionary GPUBufferDescriptor { [EnforceRange] required unsigned long long size; DOMString? note = null; };
        interface GPUQueue {};
        interface GPUBuffer {};
    "#;
    const YAML: &str = r#"
        enums: []
        bitflags: []
        structs:
          - name: buffer_descriptor
            type: extensible
            members:
              - { name: size, type: uint64 }
              - { name: note, type: nullable_string, optional: true }
        functions: []
        objects:
          - name: device
            methods:
              - name: get_queue
                returns: { type: object.queue }
              - name: create_buffer
                returns: { type: object.buffer }
                args: [{ name: descriptor, type: struct.buffer_descriptor, pointer: immutable }]
          - { name: queue, methods: [] }
          - { name: buffer, methods: [] }
    "#;
    const POLICY: &str = r#"
        schema_version = 1
        [[subset]]
        interface = "GPUDevice"
        members = ["queue", "createBuffer"]
    "#;

    #[test]
    fn public_join_inputs_covers_happy_and_error_paths() {
        let report = join_inputs(IDL, YAML, POLICY).expect("clean join");
        assert_eq!(report.interfaces.len(), 1);
        let error = join_inputs(
            IDL,
            YAML,
            "schema_version = 1\n[[subset]]\ninterface = \"GPUUnknown\"",
        )
        .expect_err("unknown policy entry");
        assert!(matches!(error, CodegenError::Policy(_)));
    }

    #[test]
    fn public_render_report_is_deterministic() {
        let report = join_inputs(IDL, YAML, POLICY).expect("join");
        let rendered = render_report(&report);
        assert!(rendered.contains("parser: weedle2 5.0.0"));
        assert!(rendered.contains("join: interfaces=1"));
    }

    #[test]
    fn public_model_items_are_directly_constructible() {
        let counts = ConstructCounts::default();
        let evidence = ParserEvidence {
            constructs: counts,
            ..ParserEvidence::default()
        };
        let value = ValueModel::default();
        let idl = IdlMemberModel {
            name: "x".to_owned(),
            kind: IdlMemberKind::Attribute,
            values: vec![value.clone()],
            same_object: true,
        };
        let c = CMemberModel {
            name: "wgpuXGetX".to_owned(),
            values: vec![value],
            callback: None,
        };
        let member = MemberPair {
            owner: "GPUX".to_owned(),
            member: "x".to_owned(),
            idl: vec![idl],
            c,
        };
        let pair = TypePair {
            idl_name: Some("GPUX".to_owned()),
            c_name: Some("WGPUX".to_owned()),
            c_chained: false,
            members: vec![member],
        };
        let mismatch = Mismatch {
            message: "difference".to_owned(),
        };
        let report = JoinReport {
            parser: evidence,
            interfaces: vec![pair],
            mismatches: vec![mismatch],
            ..JoinReport::default()
        };
        assert_eq!(
            report.interfaces[0].members[0].idl[0].kind,
            IdlMemberKind::Attribute
        );
        assert_eq!(report.mismatches[0].message, "difference");
        assert_eq!(
            CodegenError::Yaml("bad".to_owned()).to_string(),
            "YAML error: bad"
        );
    }

    #[test]
    fn parser_prepass_is_narrow_and_records_exact_text() {
        let idl = "namespace GPUFlags {\n const unsigned long ONE = 0x1;\n};\n";
        let (cooked, rewrites) = preprocess_namespace_consts(idl).expect("pre-pass");
        assert_eq!(rewrites, vec!["const unsigned long ONE = 0x1;"]);
        assert!(cooked.contains("readonly attribute unsigned long ONE;"));
    }

    #[test]
    fn names_are_derived_from_yaml_discipline() {
        assert_eq!(snake_case("BindGroupLayout"), "bind_group_layout");
        assert_eq!(c_type_name("bind_group_layout"), "WGPUBindGroupLayout");
        assert_eq!(
            c_function_name("device", "create_bind_group_layout"),
            "wgpuDeviceCreateBindGroupLayout"
        );
    }
}
