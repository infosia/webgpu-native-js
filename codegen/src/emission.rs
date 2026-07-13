//! Deterministic Rust emission for policy-selected descriptor conversions.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::{
    ChainPolicy, CodegenError, DescriptorEntry, DictOrSequenceUnionPolicy, HandleOrEnumUnionPolicy,
    JoinReport, MemberPair, Policy, SkipPolicy, TypePair, UnionFlattenPolicy, ValueModel,
};

pub(crate) fn emit_namespaces(report: &JoinReport) -> String {
    if report.namespaces.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    output.push_str(
        "pub(super) fn register_generated_namespaces<E: JsEngine>(\n    cx: E::Context<'_>,\n) -> Result<(), E::Error> {\n    let global = E::global(cx);\n",
    );
    for namespace in &report.namespaces {
        output.push_str("    {\n        let namespace = E::new_object(cx)?;\n");
        for constant in &namespace.constants {
            let _ = writeln!(
                output,
                "        let value = E::number(cx, {}.0)?;",
                constant.value
            );
            let _ = writeln!(
                output,
                "        E::define_data_property(cx, namespace, \"{}\", value, false, true, false)?;",
                constant.name
            );
        }
        let _ = writeln!(
            output,
            "        E::define_data_property(cx, global, \"{}\", namespace, true, false, true)?;",
            namespace.name
        );
        output.push_str("    }\n");
    }
    output.push_str("    Ok(())\n}\n");
    output
}

/// Emits all descriptor conversions selected by `policy` from `report`.
///
/// The descriptor name, member names, coercions, defaults, integer widths,
/// nested types, and enum values are taken from policy and the joined model.
/// Unsupported shapes are rejected instead of being approximated.
pub fn emit_conversions(report: &JoinReport, policy: &str) -> Result<String, CodegenError> {
    let policy = parse_policy(policy)?;
    validate_policy(report, &policy)?;

    let descriptors: BTreeMap<&str, &DescriptorEntry> = policy
        .descriptor
        .iter()
        .map(|entry| (entry.dictionary.as_str(), entry))
        .collect();
    let unions: BTreeMap<&str, &DictOrSequenceUnionPolicy> = policy
        .dict_or_sequence_union
        .iter()
        .map(|entry| (entry.typedef.as_str(), entry))
        .collect();
    let mut output = String::new();
    for (index, descriptor) in policy.descriptor.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        let pair = descriptor_pair(report, &descriptor.dictionary)?;
        output.push_str(&emit_descriptor(
            report,
            pair,
            descriptor,
            &descriptors,
            &unions,
        )?);
    }
    for union in &policy.dict_or_sequence_union {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&emit_dict_or_sequence_union(report, union, &descriptors)?);
    }
    let enum_conversions = emit_operation_enum_conversions(report)?;
    if !enum_conversions.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&enum_conversions);
    }
    let feature_reverse = emit_reverse_enums(report, &policy.reverse_enum);
    if !feature_reverse.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&feature_reverse);
    }
    Ok(output)
}

fn emit_reverse_enums(report: &JoinReport, selected: &[String]) -> String {
    let mut output = String::new();
    for name in selected {
        let Some(pair) = enum_pair(report, name) else {
            continue;
        };
        let Some(c_type) = pair.c_name.as_deref() else {
            continue;
        };
        let function = format!("{}_to_str", snake_case(name.trim_start_matches("GPU")));
        let convert_function = format!("convert_{}", snake_case(name));
        let _ = writeln!(
            output,
            "pub(super) fn {convert_function}<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<{c_type}, E::Error> {{"
        );
        output.push_str(
            "    // B6: generated WebIDL string-enum conversion rejects unknown values.\n",
        );
        output.push_str("    let arena = Arena::new();\n");
        output.push_str("    match E::to_str(cx, value, &arena)? {\n");
        for value in &pair.enum_values {
            if let (Some(idl_value), Some(c_value)) = (&value.idl_value, &value.c_value) {
                let constant = enum_constant(c_type, c_value);
                let _ = writeln!(output, "        \"{idl_value}\" => Ok({constant}),");
            }
        }
        let _ = writeln!(output, "        _ => Err(E::type_error(cx, \"{name}\")),");
        output.push_str("    }\n}\n\n");
        let _ = writeln!(
            output,
            "#[allow(non_upper_case_globals)]\npub(super) fn {function}(value: {c_type}) -> Option<&'static str> {{"
        );
        output.push_str("    match value {\n");
        for value in &pair.enum_values {
            if let (Some(idl_value), Some(c_value)) = (&value.idl_value, &value.c_value) {
                let constant = enum_constant(c_type, c_value);
                let _ = writeln!(output, "        {constant} => Some(\"{idl_value}\"),");
            }
        }
        output.push_str("        _ => None,\n    }\n}\n");
    }
    output
}

fn emit_operation_enum_conversions(report: &JoinReport) -> Result<String, CodegenError> {
    let mut selected = BTreeMap::new();
    for argument in report
        .interfaces
        .iter()
        .flat_map(|interface| interface.members.iter())
        .flat_map(|member| member.idl.iter())
        .flat_map(|overload| overload.values.iter().skip(1))
    {
        let name = argument.type_name.trim_end_matches('?');
        if let Some(pair) = enum_pair(report, name) {
            selected.insert(name, pair);
        }
    }

    let mut output = String::new();
    for (index, (name, pair)) in selected.into_iter().enumerate() {
        let c_type = pair
            .c_name
            .as_deref()
            .ok_or_else(|| unsupported_shape("operation argument", name, "enum has no C type"))?;
        if index != 0 {
            output.push('\n');
        }
        let function = format!("convert_{}", snake_case(name));
        let _ = writeln!(
            output,
            "pub(super) fn {function}<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<{c_type}, E::Error> {{"
        );
        output.push_str(
            "    // B6: generated WebIDL string-enum conversion rejects unknown values.\n",
        );
        output.push_str("    let arena = Arena::new();\n");
        output.push_str("    match E::to_str(cx, value, &arena)? {\n");
        for enum_value in &pair.enum_values {
            if let (Some(idl_value), Some(c_value)) = (&enum_value.idl_value, &enum_value.c_value) {
                let constant = enum_constant(c_type, c_value);
                let _ = writeln!(output, "        \"{idl_value}\" => Ok({constant}),");
            }
        }
        let _ = writeln!(output, "        _ => Err(E::type_error(cx, \"{name}\")),");
        output.push_str("    }\n}\n");
    }
    Ok(output)
}

pub(crate) fn validate_policy(report: &JoinReport, policy: &Policy) -> Result<(), CodegenError> {
    let selected: BTreeSet<&str> = policy
        .descriptor
        .iter()
        .map(|entry| entry.dictionary.as_str())
        .collect();
    let union_typedefs: BTreeSet<&str> = policy
        .dict_or_sequence_union
        .iter()
        .map(|entry| entry.typedef.as_str())
        .collect();
    for descriptor in &policy.descriptor {
        let pair = descriptor_pair(report, &descriptor.dictionary)?;
        validate_descriptor_policy(report, pair, descriptor, &selected, &union_typedefs)?;
    }
    validate_dict_or_sequence_unions(report, policy)?;
    validate_enum_value_skips(report, policy)?;
    for interface in &report.interfaces {
        for member in &interface.members {
            for overload in &member.idl {
                for argument in overload.values.iter().skip(1) {
                    let dictionary = argument.type_name.trim_end_matches('?');
                    if dictionary.ends_with("Descriptor")
                        && report
                            .dictionaries
                            .iter()
                            .any(|pair| pair.idl_name.as_deref() == Some(dictionary))
                        && !selected.contains(dictionary)
                    {
                        return Err(CodegenError::Policy(format!(
                            "unpoliced descriptor argument {}.{}: {dictionary}",
                            member.owner, member.member
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_enum_value_skips(report: &JoinReport, policy: &Policy) -> Result<(), CodegenError> {
    let mut generated_enums = BTreeSet::new();
    for argument in report
        .interfaces
        .iter()
        .flat_map(|interface| interface.members.iter())
        .flat_map(|member| member.idl.iter())
        .flat_map(|overload| overload.values.iter().skip(1))
    {
        let name = argument.type_name.trim_end_matches('?');
        if enum_pair(report, name).is_some() {
            generated_enums.insert(name);
        }
    }
    for descriptor in &policy.descriptor {
        let pair = descriptor_pair(report, &descriptor.dictionary)?;
        for value in pair
            .members
            .iter()
            .flat_map(|member| member.idl.iter())
            .flat_map(|member| member.values.iter())
            .chain(
                pair.idl_only_members
                    .iter()
                    .flat_map(|member| member.values.iter()),
            )
        {
            let name = value.type_name.trim_end_matches('?');
            if enum_pair(report, name).is_some() {
                generated_enums.insert(name);
            }
        }
        generated_enums.extend(
            descriptor
                .handle_or_enum_unions
                .iter()
                .map(|entry| entry.enum_type.as_str()),
        );
    }

    let mut covered = BTreeSet::new();
    for skip in &policy.enum_value_skip {
        if skip.reason.trim().is_empty() {
            return Err(CodegenError::Policy(format!(
                "enum-value skip {}.{} has an empty reason",
                skip.r#enum, skip.value
            )));
        }
        if !covered.insert((skip.r#enum.as_str(), skip.value.as_str())) {
            return Err(CodegenError::Policy(format!(
                "duplicate enum-value skip {}.{}",
                skip.r#enum, skip.value
            )));
        }
        if !generated_enums.contains(skip.r#enum.as_str()) {
            return Err(CodegenError::Policy(format!(
                "dead enum-value skip {}.{}: enum is not generated",
                skip.r#enum, skip.value
            )));
        }
        let pair = enum_pair(report, &skip.r#enum).ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead enum-value skip {}.{}: enum is not joined",
                skip.r#enum, skip.value
            ))
        })?;
        if !pair.enum_values.iter().any(|value| {
            value.idl_value.as_deref() == Some(skip.value.as_str()) && value.c_value.is_none()
        }) {
            return Err(CodegenError::Policy(format!(
                "dead enum-value skip {}.{}: value is not IDL-only",
                skip.r#enum, skip.value
            )));
        }
    }

    for name in generated_enums {
        let pair = enum_pair(report, name).ok_or_else(|| {
            CodegenError::Policy(format!(
                "generated enum {name} disappeared before validation"
            ))
        })?;
        for value in &pair.enum_values {
            if let (Some(idl_value), None) = (&value.idl_value, &value.c_value) {
                if !covered.contains(&(name, idl_value.as_str())) {
                    return Err(CodegenError::Policy(format!(
                        "unpoliced IDL-only value on generated enum {name}: {idl_value}"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn parse_policy(policy: &str) -> Result<Policy, CodegenError> {
    let policy: Policy =
        toml::from_str(policy).map_err(|error| CodegenError::Policy(error.to_string()))?;
    if policy.schema_version != 1 {
        return Err(CodegenError::Policy(format!(
            "unsupported schema_version {}; expected 1",
            policy.schema_version
        )));
    }
    Ok(policy)
}

fn descriptor_pair<'a>(
    report: &'a JoinReport,
    dictionary: &str,
) -> Result<&'a TypePair, CodegenError> {
    report
        .dictionaries
        .iter()
        .find(|pair| pair.idl_name.as_deref() == Some(dictionary))
        .ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead descriptor {dictionary}: it is not in the joined subset"
            ))
        })
}

fn validate_dict_or_sequence_unions(
    report: &JoinReport,
    policy: &Policy,
) -> Result<(), CodegenError> {
    for union in &policy.dict_or_sequence_union {
        let alias = report.enums.iter().any(|pair| {
            pair.idl_name
                .as_deref()
                .is_some_and(|name| name.starts_with(&format!("{} = ", union.typedef)))
        });
        if !alias {
            return Err(CodegenError::Policy(format!(
                "dict-or-sequence union {} is not joined",
                union.typedef
            )));
        }
        let pair = descriptor_pair(report, &union.dictionary)?;
        let target = pair.c_name.as_deref().ok_or_else(|| {
            CodegenError::Policy(format!(
                "dict-or-sequence union {} dictionary {} has no joined C target",
                union.typedef, union.dictionary
            ))
        })?;
        if pair.members.len() != union.max_length
            || !pair.c_only_members.is_empty()
            || !pair.idl_only_members.is_empty()
        {
            return Err(CodegenError::Policy(format!(
                "dict-or-sequence union {} does not join one-to-one to {target}",
                union.typedef
            )));
        }
        let numeric_kind = pair
            .members
            .first()
            .and_then(|member| member.idl.first())
            .and_then(|member| member.values.first())
            .map(|value| value.type_name.as_str())
            .unwrap_or_default();
        for (index, member) in pair.members.iter().enumerate() {
            let (idl, c) = member_values(member, &union.dictionary)?;
            let valid = match numeric_kind {
                "GPUIntegerCoordinate" => {
                    idl.type_name == "GPUIntegerCoordinate"
                        && idl.enforce_range
                        && idl.integer_width == Some(32)
                        && c.integer_width == Some(32)
                }
                "double" => idl.type_name == "double" && c.type_name == "double",
                _ => false,
            };
            if !valid {
                return Err(unsupported_shape(
                    &union.typedef,
                    &member.member,
                    "dict-or-sequence fields must be homogeneous GPUIntegerCoordinate or double values",
                ));
            }
            if index >= union.min_length && idl.default_value.is_none() {
                return Err(unsupported_shape(
                    &union.typedef,
                    &member.member,
                    "trailing sequence field has no dictionary default",
                ));
            }
        }
    }
    Ok(())
}

fn validate_descriptor_policy(
    report: &JoinReport,
    pair: &TypePair,
    descriptor: &DescriptorEntry,
    selected: &BTreeSet<&str>,
    union_typedefs: &BTreeSet<&str>,
) -> Result<(), CodegenError> {
    if pair.c_name.is_none() && descriptor.target.is_none() {
        return Err(CodegenError::Policy(format!(
            "descriptor {} has no joined C-ABI type or target override",
            descriptor.dictionary
        )));
    }
    if let Some(target) = &descriptor.target {
        validate_identifier(
            &descriptor.dictionary,
            "target",
            target,
            "descriptor target",
        )?;
    }
    if descriptor.zero.as_ref().is_some_and(Vec::is_empty) {
        return Err(CodegenError::Policy(format!(
            "dead zero policy {}: entry is empty",
            descriptor.dictionary
        )));
    }

    let strings = unique_entries(
        &descriptor.dictionary,
        "string",
        descriptor.strings.iter().map(|entry| entry.member.as_str()),
    )?;
    let unsupported = unique_entries(
        &descriptor.dictionary,
        "unsupported",
        descriptor.unsupported.iter().map(String::as_str),
    )?;
    let zero = unique_entries(
        &descriptor.dictionary,
        "zero",
        descriptor.zero.iter().flatten().map(String::as_str),
    )?;
    let default_empty = unique_entries(
        &descriptor.dictionary,
        "default-empty sequence",
        descriptor.default_empty_sequence.iter().map(String::as_str),
    )?;
    let skips = unique_entries(
        &descriptor.dictionary,
        "skip",
        descriptor.skips.iter().map(|entry| entry.member.as_str()),
    )?;
    let handles = unique_entries(
        &descriptor.dictionary,
        "handle",
        descriptor.handles.iter().map(|entry| entry.member.as_str()),
    )?;
    let handle_sequences = unique_entries(
        &descriptor.dictionary,
        "handle sequence",
        descriptor
            .handle_sequences
            .iter()
            .map(|entry| entry.member.as_str()),
    )?;
    let union_flatten = unique_entries(
        &descriptor.dictionary,
        "union flatten",
        descriptor
            .union_flatten
            .iter()
            .map(|entry| entry.member.as_str()),
    )?;
    let chains = unique_entries(
        &descriptor.dictionary,
        "chain",
        descriptor.chains.iter().map(|entry| entry.member.as_str()),
    )?;
    let handle_or_enum_unions = unique_entries(
        &descriptor.dictionary,
        "handle-or-enum union",
        descriptor
            .handle_or_enum_unions
            .iter()
            .map(|entry| entry.member.as_str()),
    )?;
    let required_defaults = unique_entries(
        &descriptor.dictionary,
        "required default",
        descriptor
            .required_defaults
            .iter()
            .map(|entry| entry.member.as_str()),
    )?;
    let absent_constants = unique_entries(
        &descriptor.dictionary,
        "absent constant",
        descriptor
            .absent_constants
            .iter()
            .map(|entry| entry.member.as_str()),
    )?;
    let sentinel_defaults = unique_entries(
        &descriptor.dictionary,
        "sentinel default",
        descriptor.sentinel_defaults.iter().map(String::as_str),
    )?;
    let embedded = unique_entries(
        &descriptor.dictionary,
        "embedded dictionary",
        descriptor
            .embedded
            .iter()
            .map(|entry| entry.member.as_str()),
    )?;
    let mut embedded_idl_members = BTreeSet::new();
    for entry in &descriptor.embedded {
        let nested = descriptor_pair(report, &entry.dictionary)?;
        if !selected.contains(entry.dictionary.as_str()) {
            return Err(CodegenError::Policy(format!(
                "embedded dictionary {}.{} names unselected descriptor {}",
                descriptor.dictionary, entry.member, entry.dictionary
            )));
        }
        let c_member = pair
            .c_only_members
            .iter()
            .find(|member| member.name == entry.member)
            .and_then(|member| member.values.first())
            .ok_or_else(|| {
                CodegenError::Policy(format!(
                    "dead embedded dictionary {}.{}: member is not C-only",
                    descriptor.dictionary, entry.member
                ))
            })?;
        if nested.c_name.as_deref() != Some(c_member.type_name.as_str()) {
            return Err(CodegenError::Policy(format!(
                "embedded dictionary {}.{} type {} disagrees with {}",
                descriptor.dictionary, entry.member, c_member.type_name, entry.dictionary
            )));
        }
        embedded_idl_members.extend(nested.members.iter().map(|member| member.member.as_str()));
    }
    let clamp_members: BTreeSet<&str> = pair
        .members
        .iter()
        .filter_map(|member| {
            member.idl[0].values[0]
                .clamp
                .then_some(member.member.as_str())
        })
        .collect();
    let mut special = BTreeSet::new();
    for (kind, entries) in [
        ("unsupported", &unsupported),
        ("skip", &skips),
        ("handle", &handles),
        ("handle sequence", &handle_sequences),
        ("union flatten", &union_flatten),
        ("chain", &chains),
        ("handle-or-enum union", &handle_or_enum_unions),
        ("required default", &required_defaults),
        ("absent constant", &absent_constants),
    ] {
        for entry in entries {
            if !special.insert(*entry) {
                return Err(CodegenError::Policy(format!(
                    "member {}.{entry} has overlapping {kind} policy",
                    descriptor.dictionary
                )));
            }
        }
    }

    for skip in &descriptor.skips {
        if skip.reason.trim().is_empty() {
            return Err(CodegenError::Policy(format!(
                "skip policy {}.{} has an empty reason",
                descriptor.dictionary, skip.member
            )));
        }
    }
    for default in &descriptor.required_defaults {
        if default.reason.trim().is_empty() {
            return Err(CodegenError::Policy(format!(
                "required default policy {}.{} has an empty reason",
                descriptor.dictionary, default.member
            )));
        }
        let idl = pair
            .members
            .iter()
            .find(|member| member.member == default.member)
            .and_then(|member| member.idl.first())
            .and_then(|member| member.values.first())
            .ok_or_else(|| {
                CodegenError::Policy(format!(
                    "dead required default policy {}.{}",
                    descriptor.dictionary, default.member
                ))
            })?;
        if !idl.required || !idl.enforce_range {
            return Err(CodegenError::Policy(format!(
                "dead required default policy {}.{}: member is not required EnforceRange",
                descriptor.dictionary, default.member
            )));
        }
    }
    for constant in &descriptor.absent_constants {
        validate_identifier(
            &descriptor.dictionary,
            &constant.member,
            &constant.value,
            "absent constant",
        )?;
        let member = pair
            .members
            .iter()
            .find(|member| member.member == constant.member)
            .ok_or_else(|| {
                CodegenError::Policy(format!(
                    "dead absent constant policy {}.{}",
                    descriptor.dictionary, constant.member
                ))
            })?;
        let (idl, c) = member_values(member, &descriptor.dictionary)?;
        let supported = idl.enforce_range || (idl.type_name == "float" && c.type_name == "float");
        if idl.required || !supported || idl.default_value.is_some() {
            return Err(CodegenError::Policy(format!(
                "dead absent constant policy {}.{}: member is not an optional numeric value without an IDL default",
                descriptor.dictionary, constant.member
            )));
        }
    }
    for handle in descriptor
        .handles
        .iter()
        .chain(descriptor.handle_sequences.iter())
    {
        validate_identifier(
            &descriptor.dictionary,
            &handle.member,
            &handle.helper,
            "helper",
        )?;
    }
    for chain in &descriptor.chains {
        validate_chain_policy(report, pair, descriptor, chain)?;
    }
    for flatten in &descriptor.union_flatten {
        validate_union_flatten_policy(report, pair, descriptor, flatten)?;
    }
    for union in &descriptor.handle_or_enum_unions {
        validate_handle_or_enum_union(report, pair, descriptor, union)?;
    }
    if let Some(wrapper) = &descriptor.wrapper {
        validate_identifier(
            &descriptor.dictionary,
            &wrapper.native_field,
            &wrapper.target,
            "wrapper target",
        )?;
        validate_identifier(
            &descriptor.dictionary,
            &wrapper.native_field,
            &wrapper.native_field,
            "wrapper native field",
        )?;
        for capture in &wrapper.captures {
            validate_identifier(
                &descriptor.dictionary,
                &capture.field,
                &capture.field,
                "wrapper capture field",
            )?;
            validate_path(
                &descriptor.dictionary,
                &capture.field,
                &capture.source,
                "wrapper capture source",
            )?;
        }
        for capture in &wrapper.sequence_captures {
            for (kind, value) in [
                ("wrapper sequence field", capture.field.as_str()),
                ("wrapper sequence source", capture.source.as_str()),
                (
                    "wrapper sequence element field",
                    capture.element_field.as_str(),
                ),
            ] {
                validate_identifier(&descriptor.dictionary, &capture.field, value, kind)?;
            }
            if !pair
                .members
                .iter()
                .any(|member| member.member == capture.source)
            {
                return Err(CodegenError::Policy(format!(
                    "dead wrapper sequence capture {}.{} from {}",
                    descriptor.dictionary, capture.field, capture.source
                )));
            }
        }
    }

    for entry in &descriptor.strings {
        let member = pair
            .members
            .iter()
            .find(|member| member.member == entry.member)
            .ok_or_else(|| {
                CodegenError::Policy(format!(
                    "dead string policy {}.{}: member is not joined",
                    descriptor.dictionary, entry.member
                ))
            })?;
        let (idl, c) = member_values(member, &descriptor.dictionary)?;
        if !is_idl_string(idl) || !c.string_view {
            return Err(CodegenError::Policy(format!(
                "dead string policy {}.{}: member is not a joined string",
                descriptor.dictionary, entry.member
            )));
        }
        if entry.nullable != idl.nullable {
            return Err(CodegenError::Policy(format!(
                "string nullability disagreement for {}.{}: policy={}, IDL={}",
                descriptor.dictionary, entry.member, entry.nullable, idl.nullable
            )));
        }
        let c_nullable = idl.nullable || (!idl.required && idl.default_value.is_none());
        if c.nullable != c_nullable {
            return Err(CodegenError::Policy(format!(
                "string nullability disagreement for {}.{}: C-ABI={}, expected={}",
                descriptor.dictionary, entry.member, c.nullable, c_nullable
            )));
        }
    }

    for member in &pair.members {
        let (idl, c) = member_values(member, &descriptor.dictionary)?;
        let name = member.member.as_str();
        if is_idl_string(idl) && c.string_view && !strings.contains(name) {
            return Err(CodegenError::Policy(format!(
                "unpoliced string nullability for {}.{name}",
                descriptor.dictionary
            )));
        }
        if idl.clamp {
            if idl.enforce_range || idl.integer_width != Some(16) || c.integer_width != Some(16) {
                return Err(CodegenError::Policy(format!(
                    "unsupported Clamp shape {}.{name}: IDL={} C-ABI={}",
                    descriptor.dictionary, idl.type_name, c.type_name
                )));
            }
            continue;
        }
        if unsupported.contains(name) {
            if !is_dictionary(report, &idl.type_name) || c.default_value.as_deref() != Some("zero")
            {
                return Err(CodegenError::Policy(format!(
                    "dead unsupported policy {}.{name}: joined member is not a zero-default nested dictionary",
                    descriptor.dictionary
                )));
            }
            continue;
        }
        if skips.contains(name) {
            continue;
        }
        if handles.contains(name) {
            if !(idl.type_name.trim_end_matches('?').starts_with("GPU")
                || idl.type_name.starts_with("(GPU"))
                || !c.type_name.starts_with("WGPU")
                || c.count_and_pointer
            {
                return Err(CodegenError::Policy(format!(
                    "dead handle policy {}.{name}: member is not a joined handle",
                    descriptor.dictionary
                )));
            }
            continue;
        }
        if handle_sequences.contains(name) {
            if sequence_element(&idl.type_name).is_none() || !c.count_and_pointer {
                return Err(CodegenError::Policy(format!(
                    "dead handle sequence policy {}.{name}: member is not a joined sequence/count-pointer",
                    descriptor.dictionary
                )));
            }
            continue;
        }
        if handle_or_enum_unions.contains(name) {
            continue;
        }
        if union_typedefs.contains(idl.type_name.as_str()) {
            continue;
        }
        if is_dictionary(report, &idl.type_name) && !selected.contains(idl.type_name.as_str()) {
            return Err(CodegenError::Policy(format!(
                "unpoliced unsupported nested dictionary {}.{name}",
                descriptor.dictionary
            )));
        }
        if default_empty.contains(name) && !is_sequence(&idl.type_name) {
            return Err(CodegenError::Policy(format!(
                "dead default-empty sequence policy {}.{name}: member is not a sequence",
                descriptor.dictionary
            )));
        }
    }

    for member in &pair.idl_only_members {
        if !unsupported.contains(member.name.as_str())
            && !skips.contains(member.name.as_str())
            && !union_flatten.contains(member.name.as_str())
            && !chains.contains(member.name.as_str())
            && !embedded_idl_members.contains(member.name.as_str())
        {
            return Err(CodegenError::Policy(format!(
                "unpoliced IDL-only member {}.{}",
                descriptor.dictionary, member.name
            )));
        }
    }
    for name in &unsupported {
        let joined = pair.members.iter().any(|member| member.member == *name);
        let idl_only = pair
            .idl_only_members
            .iter()
            .any(|member| member.name == *name);
        if !joined && !idl_only {
            return Err(CodegenError::Policy(format!(
                "dead unsupported policy {}.{name}: member is not in WebIDL",
                descriptor.dictionary
            )));
        }
    }

    for (kind, names) in [
        ("skip", &skips),
        ("handle", &handles),
        ("handle sequence", &handle_sequences),
        ("union flatten", &union_flatten),
        ("chain", &chains),
        ("handle-or-enum union", &handle_or_enum_unions),
    ] {
        for name in names {
            let joined = pair.members.iter().any(|member| member.member == *name);
            let idl_only = pair
                .idl_only_members
                .iter()
                .any(|member| member.name == *name);
            if !joined && !idl_only {
                return Err(CodegenError::Policy(format!(
                    "dead {kind} policy {}.{name}: member is not in WebIDL",
                    descriptor.dictionary
                )));
            }
        }
    }

    let flattened_c: BTreeSet<String> = descriptor
        .union_flatten
        .iter()
        .flat_map(|entry| {
            entry
                .fields
                .iter()
                .map(|field| field.c_member.clone())
                .chain(entry.handle_arms.iter().map(|interface| {
                    snake_case(
                        interface
                            .strip_prefix("GPU")
                            .filter(|value| !value.is_empty())
                            .unwrap_or(interface),
                    )
                }))
                .chain(entry.zero_c_members.iter().cloned())
        })
        .collect();

    for member in &pair.c_only_members {
        if !zero.contains(member.name.as_str())
            && !flattened_c.contains(member.name.as_str())
            && !embedded.contains(member.name.as_str())
        {
            return Err(CodegenError::Policy(format!(
                "unpoliced C-only member {}.{}",
                descriptor.dictionary, member.name
            )));
        }
        if flattened_c.contains(member.name.as_str()) {
            continue;
        }
        let value = member.values.first().ok_or_else(|| {
            unsupported_shape(&descriptor.dictionary, &member.name, "missing C value")
        })?;
        if value.default_value.is_some()
            && value.default_value.as_deref() != Some("0")
            && value.default_value.as_deref() != Some("zero")
            && value.default_value.as_deref() != Some("none")
        {
            return Err(CodegenError::Policy(format!(
                "zero policy {}.{} disagrees with C-ABI default {:?}",
                descriptor.dictionary, member.name, value.default_value
            )));
        }
    }
    for name in &zero {
        if !pair
            .c_only_members
            .iter()
            .any(|member| member.name == *name)
        {
            return Err(CodegenError::Policy(format!(
                "dead zero policy {}.{name}: member is not C-only",
                descriptor.dictionary
            )));
        }
    }
    for name in &default_empty {
        if !pair.members.iter().any(|member| member.member == *name) {
            return Err(CodegenError::Policy(format!(
                "dead default-empty sequence policy {}.{name}: member is not joined",
                descriptor.dictionary
            )));
        }
    }
    for name in &sentinel_defaults {
        let idl = pair
            .members
            .iter()
            .find(|member| member.member == *name)
            .and_then(|member| member.idl.first())
            .and_then(|member| member.values.first())
            .ok_or_else(|| {
                CodegenError::Policy(format!(
                    "dead sentinel default policy {}.{name}",
                    descriptor.dictionary
                ))
            })?;
        let enum_pair = enum_pair(report, &idl.type_name).ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead sentinel default policy {}.{name}: member is not an enum",
                descriptor.dictionary
            ))
        })?;
        if idl.required
            || idl.default_value.is_none()
            || !enum_pair.enum_values.iter().any(|value| {
                value.idl_value.is_none()
                    && value
                        .c_value
                        .as_deref()
                        .is_some_and(|value| canonical(value) == "undefined")
            })
        {
            return Err(CodegenError::Policy(format!(
                "dead sentinel default policy {}.{name}: no optional IDL default/C undefined pair",
                descriptor.dictionary
            )));
        }
    }
    for name in clamp_members {
        if special.contains(name) {
            return Err(CodegenError::Policy(format!(
                "Clamp member {}.{name} has overlapping policy",
                descriptor.dictionary
            )));
        }
    }
    Ok(())
}

fn unique_entries<'a>(
    dictionary: &str,
    kind: &str,
    entries: impl Iterator<Item = &'a str>,
) -> Result<BTreeSet<&'a str>, CodegenError> {
    let mut unique = BTreeSet::new();
    for entry in entries {
        if !unique.insert(entry) {
            return Err(CodegenError::Policy(format!(
                "duplicate {kind} policy {dictionary}.{entry}"
            )));
        }
    }
    Ok(unique)
}

fn validate_identifier(
    dictionary: &str,
    member: &str,
    value: &str,
    kind: &str,
) -> Result<(), CodegenError> {
    let mut chars = value.chars();
    if !chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        || !chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return Err(CodegenError::Policy(format!(
            "invalid {kind} identifier for {dictionary}.{member}: {value}"
        )));
    }
    Ok(())
}

fn validate_path(
    dictionary: &str,
    member: &str,
    value: &str,
    kind: &str,
) -> Result<(), CodegenError> {
    if value.is_empty() {
        return Err(CodegenError::Policy(format!(
            "invalid {kind} for {dictionary}.{member}: empty path"
        )));
    }
    for segment in value.split('.') {
        validate_identifier(dictionary, member, segment, kind)?;
    }
    Ok(())
}

fn validate_chain_policy(
    report: &JoinReport,
    pair: &TypePair,
    descriptor: &DescriptorEntry,
    chain: &ChainPolicy,
) -> Result<(), CodegenError> {
    if chain.reason.trim().is_empty() {
        return Err(CodegenError::Policy(format!(
            "chain policy {}.{} has an empty reason",
            descriptor.dictionary, chain.member
        )));
    }
    validate_identifier(
        &descriptor.dictionary,
        &chain.member,
        &chain.target,
        "chain target",
    )?;
    validate_identifier(
        &descriptor.dictionary,
        &chain.member,
        &chain.s_type,
        "sType",
    )?;
    let idl = pair
        .idl_only_members
        .iter()
        .find(|member| member.name == chain.member)
        .and_then(|member| member.values.first())
        .ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead chain policy {}.{}: member is not IDL-only",
                descriptor.dictionary, chain.member
            ))
        })?;
    let target = report
        .dictionaries
        .iter()
        .find(|candidate| candidate.c_name.as_deref() == Some(chain.target.as_str()))
        .ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead chain policy {}.{}: target {} is not joined",
                descriptor.dictionary, chain.member, chain.target
            ))
        })?;
    if !target.c_chained {
        return Err(CodegenError::Policy(format!(
            "dead chain policy {}.{}: target {} is not chained",
            descriptor.dictionary, chain.member, chain.target
        )));
    }
    let field = target
        .c_only_members
        .iter()
        .find(|member| member.name == chain.field)
        .and_then(|member| member.values.first())
        .ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead chain policy {}.{}: target field {}.{} does not exist",
                descriptor.dictionary, chain.member, chain.target, chain.field
            ))
        })?;
    let required_string = is_idl_string(idl) && idl.required && field.string_view;
    let optional_u64 = !idl.required
        && idl.enforce_range
        && idl.integer_width == Some(64)
        && field.integer_width == Some(64)
        && idl.default_value.is_some()
        && idl.default_value == field.default_value;
    if !required_string && !optional_u64 {
        return Err(CodegenError::Policy(format!(
            "dead chain policy {}.{}: source and target field are not a supported required string or optional EnforceRange u64 pair",
            descriptor.dictionary, chain.member,
        )));
    }
    Ok(())
}

fn validate_union_flatten_policy(
    report: &JoinReport,
    pair: &TypePair,
    descriptor: &DescriptorEntry,
    policy: &UnionFlattenPolicy,
) -> Result<(), CodegenError> {
    let union = pair
        .idl_only_members
        .iter()
        .find(|member| member.name == policy.member)
        .and_then(|member| member.values.first())
        .ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead union flatten policy {}.{}: member is not IDL-only",
                descriptor.dictionary, policy.member
            ))
        })?;
    if union.type_name != policy.union_type || policy.unsupported_error.trim().is_empty() {
        return Err(CodegenError::Policy(format!(
            "union flatten policy {}.{} disagrees with IDL union {}",
            descriptor.dictionary, policy.member, union.type_name
        )));
    }
    let arm = report
        .dictionaries
        .iter()
        .find(|candidate| candidate.idl_name.as_deref() == Some(policy.arm.as_str()))
        .ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead union flatten policy {}.{}: arm {} is not joined",
                descriptor.dictionary, policy.member, policy.arm
            ))
        })?;
    let mut claimed_idl = BTreeSet::new();
    let mut claimed_c = BTreeSet::new();
    for interface in &policy.handle_arms {
        let object = interface
            .strip_prefix("GPU")
            .filter(|value| !value.is_empty())
            .unwrap_or(interface);
        let c_member = snake_case(object);
        if !claimed_c.insert(c_member.clone()) {
            return Err(CodegenError::Policy(format!(
                "duplicate flattened handle arm {}.{}.{}",
                descriptor.dictionary, policy.member, interface
            )));
        }
        let joined = report.interfaces.iter().any(|candidate| {
            candidate.idl_name.as_deref() == Some(interface.as_str()) && candidate.c_name.is_some()
        });
        if !joined {
            return Err(CodegenError::Policy(format!(
                "dead flattened handle arm {}.{}.{}: interface is not joined",
                descriptor.dictionary, policy.member, interface
            )));
        }
        if !pair
            .c_only_members
            .iter()
            .any(|member| member.name == c_member)
        {
            return Err(CodegenError::Policy(format!(
                "dead flattened handle arm {}.{}.{}: C field {} does not exist",
                descriptor.dictionary, policy.member, interface, c_member
            )));
        }
    }
    for direct in &policy.direct_handle_arms {
        let joined = report.interfaces.iter().any(|candidate| {
            candidate.idl_name.as_deref() == Some(direct.interface.as_str())
                && candidate.c_name.is_some()
        });
        if !joined {
            return Err(CodegenError::Policy(format!(
                "dead direct flattened handle arm {}.{}.{}: interface is not joined",
                descriptor.dictionary, policy.member, direct.interface
            )));
        }
        if !pair
            .c_only_members
            .iter()
            .any(|member| member.name == direct.c_member)
        {
            return Err(CodegenError::Policy(format!(
                "dead direct flattened handle arm {}.{}.{}: C field {} does not exist",
                descriptor.dictionary, policy.member, direct.interface, direct.c_member
            )));
        }
        if direct.payload_field.is_some() == direct.handle_helper.is_some() {
            return Err(CodegenError::Policy(format!(
                "direct flattened handle arm {}.{}.{} needs exactly one payload field or handle helper",
                descriptor.dictionary, policy.member, direct.interface
            )));
        }
        if let Some(field) = &direct.payload_field {
            validate_identifier(
                &descriptor.dictionary,
                &policy.member,
                field,
                "direct handle payload field",
            )?;
        }
        if let Some(helper) = &direct.handle_helper {
            validate_identifier(
                &descriptor.dictionary,
                &policy.member,
                helper,
                "direct handle helper",
            )?;
        }
        if let Some(dispatch) = &direct.creator_dispatch {
            validate_identifier(
                &descriptor.dictionary,
                &policy.member,
                dispatch,
                "direct handle creator dispatch",
            )?;
            if direct.created_capture.is_none() {
                return Err(CodegenError::Policy(format!(
                    "created direct handle arm {}.{}.{} has no created capture",
                    descriptor.dictionary, policy.member, direct.interface
                )));
            }
        } else if direct.created_capture.is_some() {
            return Err(CodegenError::Policy(format!(
                "borrowed direct handle arm {}.{}.{} has a created capture",
                descriptor.dictionary, policy.member, direct.interface
            )));
        }
    }
    for field in &policy.fields {
        if !claimed_idl.insert(field.member.as_str()) || !claimed_c.insert(field.c_member.clone()) {
            return Err(CodegenError::Policy(format!(
                "duplicate flattened field policy {}.{}.{}",
                descriptor.dictionary, policy.member, field.member
            )));
        }
        if let Some(helper) = &field.handle_helper {
            validate_identifier(
                &descriptor.dictionary,
                &field.member,
                helper,
                "handle helper",
            )?;
        }
        if let Some(constant) = &field.absent_constant {
            validate_identifier(
                &descriptor.dictionary,
                &field.member,
                constant,
                "absent constant",
            )?;
        }
        let idl = idl_dictionary_value(arm, &field.member).ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead flattened field {}.{}.{}: arm member does not exist",
                descriptor.dictionary, policy.member, field.member
            ))
        })?;
        if field.handle_helper.is_none() && !idl.enforce_range {
            return Err(CodegenError::Policy(format!(
                "dead flattened field {}.{}.{}: non-handle field is not EnforceRange",
                descriptor.dictionary, policy.member, field.member
            )));
        }
        if !pair
            .c_only_members
            .iter()
            .any(|member| member.name == field.c_member)
        {
            return Err(CodegenError::Policy(format!(
                "dead flattened C field {}.{}.{}",
                descriptor.dictionary, policy.member, field.c_member
            )));
        }
    }
    for name in &policy.zero_c_members {
        if !claimed_c.insert(name.clone())
            || !pair
                .c_only_members
                .iter()
                .any(|member| member.name == *name)
        {
            return Err(CodegenError::Policy(format!(
                "dead flattened zero field {}.{}.{}",
                descriptor.dictionary, policy.member, name
            )));
        }
    }
    let all_c: BTreeSet<String> = pair
        .c_only_members
        .iter()
        .map(|member| member.name.clone())
        .collect();
    if claimed_c != all_c {
        return Err(CodegenError::Policy(format!(
            "unpoliced flattened C fields for {}.{}: expected {:?}, got {:?}",
            descriptor.dictionary, policy.member, all_c, claimed_c
        )));
    }
    Ok(())
}

fn validate_handle_or_enum_union(
    report: &JoinReport,
    pair: &TypePair,
    descriptor: &DescriptorEntry,
    policy: &HandleOrEnumUnionPolicy,
) -> Result<(), CodegenError> {
    if policy.reason.trim().is_empty() {
        return Err(CodegenError::Policy(format!(
            "handle-or-enum union policy {}.{} has an empty reason",
            descriptor.dictionary, policy.member
        )));
    }
    validate_identifier(
        &descriptor.dictionary,
        &policy.member,
        &policy.handle_helper,
        "handle helper",
    )?;
    let idl = pair
        .members
        .iter()
        .find(|member| member.member == policy.member)
        .and_then(|member| member.idl.first())
        .and_then(|member| member.values.first())
        .ok_or_else(|| {
            CodegenError::Policy(format!(
                "dead handle-or-enum union policy {}.{}",
                descriptor.dictionary, policy.member
            ))
        })?;
    if idl.type_name != policy.union_type
        || !policy.union_type.contains(&policy.handle_type)
        || !policy.union_type.contains(&policy.enum_type)
    {
        return Err(CodegenError::Policy(format!(
            "handle-or-enum union policy {}.{} disagrees with IDL type {}",
            descriptor.dictionary, policy.member, idl.type_name
        )));
    }
    if !idl.required || idl.nullable {
        return Err(CodegenError::Policy(format!(
            "handle-or-enum union policy {}.{} requires a required, non-null WebIDL member",
            descriptor.dictionary, policy.member
        )));
    }
    let enum_pair = enum_pair(report, &policy.enum_type).ok_or_else(|| {
        CodegenError::Policy(format!(
            "dead handle-or-enum union policy {}.{}: enum {} is not joined",
            descriptor.dictionary, policy.member, policy.enum_type
        ))
    })?;
    if !enum_pair
        .enum_values
        .iter()
        .any(|value| value.idl_value.as_deref() == Some(policy.enum_value.as_str()))
    {
        return Err(CodegenError::Policy(format!(
            "dead handle-or-enum union policy {}.{}: enum value {} is not joined",
            descriptor.dictionary, policy.member, policy.enum_value
        )));
    }
    Ok(())
}

fn emit_descriptor(
    report: &JoinReport,
    pair: &TypePair,
    descriptor: &DescriptorEntry,
    descriptors: &BTreeMap<&str, &DescriptorEntry>,
    unions: &BTreeMap<&str, &DictOrSequenceUnionPolicy>,
) -> Result<String, CodegenError> {
    let dictionary = &descriptor.dictionary;
    let target = descriptor
        .target
        .as_deref()
        .or(pair.c_name.as_deref())
        .ok_or_else(|| CodegenError::Policy(format!("descriptor {dictionary} has no target")))?;
    let raw_c = pair.c_name.as_deref() == Some(target);
    let return_target = descriptor
        .wrapper
        .as_ref()
        .map_or(target, |wrapper| wrapper.target.as_str());
    let function = format!(
        "convert_{}",
        snake_case(dictionary.strip_prefix("GPU").unwrap_or(dictionary))
    );
    let string_policy: BTreeMap<&str, bool> = descriptor
        .strings
        .iter()
        .map(|entry| (entry.member.as_str(), entry.nullable))
        .collect();
    let unsupported: BTreeSet<&str> = descriptor.unsupported.iter().map(String::as_str).collect();
    let skips: BTreeMap<&str, &SkipPolicy> = descriptor
        .skips
        .iter()
        .map(|entry| (entry.member.as_str(), entry))
        .collect();
    let handles: BTreeMap<&str, &crate::HandlePolicy> = descriptor
        .handles
        .iter()
        .map(|entry| (entry.member.as_str(), entry))
        .collect();
    let handle_sequences: BTreeMap<&str, &str> = descriptor
        .handle_sequences
        .iter()
        .map(|entry| (entry.member.as_str(), entry.helper.as_str()))
        .collect();
    let flatten: BTreeMap<&str, &UnionFlattenPolicy> = descriptor
        .union_flatten
        .iter()
        .map(|entry| (entry.member.as_str(), entry))
        .collect();
    let chains: BTreeMap<&str, &ChainPolicy> = descriptor
        .chains
        .iter()
        .map(|entry| (entry.member.as_str(), entry))
        .collect();
    let handle_or_enum: BTreeMap<&str, &HandleOrEnumUnionPolicy> = descriptor
        .handle_or_enum_unions
        .iter()
        .map(|entry| (entry.member.as_str(), entry))
        .collect();
    let required_defaults: BTreeMap<&str, u64> = descriptor
        .required_defaults
        .iter()
        .map(|entry| (entry.member.as_str(), entry.value))
        .collect();
    let absent_constants: BTreeMap<&str, &str> = descriptor
        .absent_constants
        .iter()
        .map(|entry| (entry.member.as_str(), entry.value.as_str()))
        .collect();
    let sentinel_defaults: BTreeSet<&str> = descriptor
        .sentinel_defaults
        .iter()
        .map(String::as_str)
        .collect();
    let default_empty: BTreeSet<&str> = descriptor
        .default_empty_sequence
        .iter()
        .map(String::as_str)
        .collect();
    let embedded: BTreeMap<&str, &str> = descriptor
        .embedded
        .iter()
        .map(|entry| (entry.member.as_str(), entry.dictionary.as_str()))
        .collect();
    let needs_arena = descriptor_needs_arena(pair, descriptor);
    let needs_static = descriptor_needs_static(report, pair, descriptor, descriptors);

    let mut members: Vec<&MemberPair> = pair.members.iter().collect();
    if !raw_c {
        members.sort_by_key(|member| {
            let idl = &member.idl[0].values[0];
            if idl.required {
                0
            } else if is_idl_string(idl) {
                2
            } else {
                1
            }
        });
    }

    let mut output = String::new();
    let _ = writeln!(
        output,
        "/// Converts a JavaScript `{dictionary}` into `{return_target}`."
    );
    if unions
        .values()
        .any(|union| union.dictionary == descriptor.dictionary)
    {
        output.push_str("#[allow(dead_code)] // T1 emits union arms even before every typedef has an API consumer.\n");
    }
    let _ = writeln!(
        output,
        "pub(super) fn {function}<E: JsEngine{}>(",
        if needs_static { " + 'static" } else { "" }
    );
    output.push_str("    cx: E::Context<'_>,\n");
    output.push_str("    value: E::Value,\n");
    if needs_arena {
        output.push_str("    arena: &Arena,\n");
    }
    let mut created_captures: BTreeSet<_> = descriptor
        .union_flatten
        .iter()
        .flat_map(|flatten| flatten.direct_handle_arms.iter())
        .filter_map(|arm| arm.created_capture.as_deref())
        .collect();
    if let Some(capture) = descriptor.created_view_capture.as_deref() {
        created_captures.insert(capture);
    }
    for capture in &created_captures {
        let _ = writeln!(output, "    {capture}: &mut CreatedTextureViewCapture,");
    }
    let _ = writeln!(output, ") -> Result<{return_target}, E::Error> {{");

    let mut cited_required = false;
    for member in &members {
        let (idl, _) = member_values(member, dictionary)?;
        let name = &member.member;
        if let Some(skip) = skips.get(name.as_str()) {
            if skip.reject_if_present {
                let value_name = format!("{}_value", snake_case(name));
                let _ = writeln!(
                    output,
                    "    let {value_name} = dictionary_member::<E>(cx, value, \"{name}\")?;"
                );
                emit_rejected_skip_check(&mut output, name, &value_name);
            }
            continue;
        }
        let value_name = format!("{}_value", snake_case(name));
        if required_defaults.contains_key(name.as_str()) {
            let _ = writeln!(
                output,
                "    let {value_name} = dictionary_member::<E>(cx, value, \"{name}\")?;"
            );
        } else if idl.required && !default_empty.contains(name.as_str()) {
            if !cited_required {
                output.push_str("    // DR-M3: required dictionary members reject undefined.\n");
                cited_required = true;
            }
            let _ = writeln!(
                output,
                "    let {value_name} = required_member::<E>(cx, value, \"{name}\")?;"
            );
        } else {
            let _ = writeln!(
                output,
                "    let {value_name} = dictionary_member::<E>(cx, value, \"{name}\")?;"
            );
        }
        if unsupported.contains(name.as_str()) {
            emit_unsupported_check(&mut output, name, &value_name);
        }
    }
    let embedded_member_names: BTreeSet<&str> = descriptor
        .embedded
        .iter()
        .filter_map(|entry| descriptor_pair(report, &entry.dictionary).ok())
        .flat_map(|pair| pair.members.iter().map(|member| member.member.as_str()))
        .collect();
    for member in &pair.idl_only_members {
        if embedded_member_names.contains(member.name.as_str()) {
            continue;
        }
        let name = &member.name;
        if let Some(skip) = skips.get(name.as_str()) {
            if skip.reject_if_present {
                let value_name = format!("{}_value", snake_case(name));
                let _ = writeln!(
                    output,
                    "    let {value_name} = dictionary_member::<E>(cx, value, \"{name}\")?;"
                );
                emit_rejected_skip_check(&mut output, name, &value_name);
            }
            continue;
        }
        let value_name = format!("{}_value", snake_case(name));
        let idl = member
            .values
            .first()
            .ok_or_else(|| unsupported_shape(dictionary, name, "missing IDL-only member value"))?;
        if idl.required
            && (flatten.contains_key(name.as_str()) || chains.contains_key(name.as_str()))
        {
            if !cited_required {
                output.push_str("    // DR-M3: required dictionary members reject undefined.\n");
                cited_required = true;
            }
            let _ = writeln!(
                output,
                "    let {value_name} = required_member::<E>(cx, value, \"{name}\")?;"
            );
        } else {
            let _ = writeln!(
                output,
                "    let {value_name} = dictionary_member::<E>(cx, value, \"{name}\")?;"
            );
        }
        if unsupported.contains(name.as_str()) {
            emit_unsupported_check(&mut output, name, &value_name);
        }
    }

    for (member, nested) in &embedded {
        let local = rust_field_name(member, false);
        let function = format!(
            "convert_{}",
            snake_case(nested.strip_prefix("GPU").unwrap_or(nested))
        );
        let nested_pair = descriptor_pair(report, nested)?;
        let nested_descriptor = descriptors.get(nested).ok_or_else(|| {
            CodegenError::Policy(format!("embedded descriptor {nested} is not selected"))
        })?;
        if descriptor_needs_arena(nested_pair, nested_descriptor) {
            let _ = writeln!(
                output,
                "    let {local} = {function}::<E>(cx, value, arena)?;"
            );
        } else {
            let _ = writeln!(output, "    let {local} = {function}::<E>(cx, value)?;");
        }
    }

    for member in &members {
        if unsupported.contains(member.member.as_str()) {
            continue;
        }
        let (idl, c) = member_values(member, dictionary)?;
        let name = &member.member;
        let local = rust_field_name(name, false);
        let value = format!("{}_value", snake_case(name));
        if skips.contains_key(name.as_str()) {
            continue;
        }
        if let Some(handle) = handles.get(name.as_str()) {
            let helper = &handle.helper;
            if idl.required {
                if handle.accept_texture {
                    let capture = descriptor
                        .created_view_capture
                        .as_deref()
                        .expect("validated created-view capture");
                    let _ = writeln!(output, "    let {local} = if let Some(texture) = E::payload(cx, {value}, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).map(|payload| payload.texture) {{");
                    output.push_str("        let created = unsafe { (E::environment(cx).gpu().texture_create_view)(texture, ptr::null()) };\n");
                    output.push_str("        if created.is_null() { return Err(E::operation_error(cx, \"wgpuTextureCreateView returned null\")); }\n");
                    let _ = writeln!(output, "        {capture}.push(created);");
                    output.push_str("        created\n    } else {\n");
                    let _ = writeln!(output, "        {helper}::<E>(cx, {value})?");
                    output.push_str("    };\n");
                } else {
                    let _ = writeln!(output, "    let {local} = {helper}::<E>(cx, {value})?;");
                }
            } else {
                let _ = writeln!(
                    output,
                    "    let {local} = if E::is_undefined(cx, {value}) {{"
                );
                output.push_str("        ptr::null_mut()\n    } else {\n");
                if handle.accept_texture {
                    let capture = descriptor
                        .created_view_capture
                        .as_deref()
                        .expect("validated created-view capture");
                    let _ = writeln!(output, "        if let Some(texture) = E::payload(cx, {value}, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).map(|payload| payload.texture) {{");
                    output.push_str("            let created = unsafe { (E::environment(cx).gpu().texture_create_view)(texture, ptr::null()) };\n");
                    output.push_str("            if created.is_null() { return Err(E::operation_error(cx, \"wgpuTextureCreateView returned null\")); }\n");
                    let _ = writeln!(output, "            {capture}.push(created);");
                    output.push_str("            created\n        } else {\n");
                    let _ = writeln!(output, "            {helper}::<E>(cx, {value})?");
                    output.push_str("        }\n");
                } else {
                    let _ = writeln!(output, "        {helper}::<E>(cx, {value})?");
                }
                output.push_str("    };\n");
            }
        } else if let Some(helper) = handle_sequences.get(name.as_str()) {
            emit_handle_sequence_local(&mut output, name, &local, &value, helper, idl.required);
        } else if let Some(policy) = handle_or_enum.get(name.as_str()) {
            emit_handle_or_enum_local(&mut output, name, &local, &value, policy);
        } else if is_idl_string(idl) {
            emit_string_local(
                &mut output,
                dictionary,
                name,
                &local,
                &value,
                string_policy.get(name.as_str()).copied().ok_or_else(|| {
                    CodegenError::Policy(format!(
                        "missing string policy during emission for {dictionary}.{name}"
                    ))
                })?,
                idl,
            )?;
        } else if is_enum(report, &idl.type_name) {
            emit_enum_local(
                &mut output,
                report,
                dictionary,
                name,
                &value,
                idl,
                sentinel_defaults.contains(name.as_str()),
            )?;
        } else if let Some(union) = unions.get(idl.type_name.as_str()) {
            let convert = format!("convert_{}", snake_case(&union.typedef));
            if idl.required {
                let _ = writeln!(output, "    let {local} = {convert}::<E>(cx, {value})?;");
            } else {
                let _ = writeln!(
                    output,
                    "    let {local} = if E::is_undefined(cx, {value}) {{"
                );
                output.push_str("        // The pinned C initializer uses the all-zero value for an absent numeric union.\n");
                output.push_str("        unsafe { std::mem::zeroed() }\n    } else {\n");
                let _ = writeln!(output, "        {convert}::<E>(cx, {value})?");
                output.push_str("    };\n");
            }
        } else if is_dictionary(report, &idl.type_name) {
            emit_nested_local(
                &mut output,
                report,
                dictionary,
                name,
                &local,
                &value,
                idl,
                c,
                descriptors,
                descriptor.created_view_capture.as_deref(),
            )?;
        } else if let Some(element) = sequence_element(&idl.type_name) {
            emit_sequence_local(
                &mut output,
                report,
                dictionary,
                name,
                &local,
                &value,
                element,
                default_empty.contains(name.as_str()),
                descriptors,
                descriptor.created_view_capture.as_deref(),
            )?;
        } else if is_string_double_record(idl, c) {
            emit_string_double_record_local(&mut output, name, &local, &value);
        }
    }

    for member in &pair.idl_only_members {
        let name = &member.name;
        let value = format!("{}_value", snake_case(name));
        if let Some(policy) = flatten.get(name.as_str()) {
            emit_union_flatten_locals(&mut output, report, dictionary, &value, policy)?;
        } else if let Some(policy) = chains.get(name.as_str()) {
            let idl = member
                .values
                .first()
                .ok_or_else(|| unsupported_shape(dictionary, name, "missing chained IDL value"))?;
            emit_chain_local(&mut output, name, &value, idl, policy);
        }
    }

    if dictionary == "GPURenderPassColorAttachment" {
        output.push_str("    let depth_slice = if E::is_undefined(cx, depth_slice_value) {\n");
        output.push_str("        None\n");
        output.push_str("    } else {\n");
        output.push_str("        Some(enforce_u32::<E>(cx, depth_slice_value, \"depthSlice\")?)\n");
        output.push_str("    };\n");
        output.push_str(
            "    created_texture_views.check_depth_slice::<E>(cx, view_value, depth_slice)?;\n",
        );
    }

    if let Some(wrapper) = &descriptor.wrapper {
        for capture in &wrapper.captures {
            if capture.take {
                let _ = writeln!(
                    output,
                    "    let {} = {}.take();",
                    capture.field, capture.source
                );
            } else if capture.source != capture.field {
                let mut path = capture.source.split('.');
                let root = path.next().unwrap_or_default();
                let tail = path.collect::<Vec<_>>();
                let pointer_root = pair.members.iter().any(|member| {
                    member.member == root
                        && member.c.values[0].pointer.as_deref() == Some("immutable")
                });
                if pointer_root && !tail.is_empty() {
                    let access = tail.join(".");
                    let _ = writeln!(output, "    let {} = if {root}.is_null() {{", capture.field);
                    output.push_str("        ptr::null_mut()\n    } else {\n");
                    output.push_str("        // SAFETY: the arena-owned optional nested descriptor remains live through the native call.\n");
                    let _ = writeln!(output, "        unsafe {{ (*{root}).{access} }}");
                    output.push_str("    };\n");
                } else {
                    let _ = writeln!(output, "    let {} = {};", capture.field, capture.source);
                }
            }
        }
        for capture in &wrapper.sequence_captures {
            let _ = writeln!(output, "    let {} = {}", capture.field, capture.source);
            output.push_str("        .iter()\n");
            let condition = if let Some(exclude) = &capture.exclude_source {
                format!(
                    "!item.{0}.is_null() && !{exclude}.contains(&item.{0})",
                    capture.element_field
                )
            } else {
                format!("!item.{}.is_null()", capture.element_field)
            };
            let _ = writeln!(
                output,
                "        .filter_map(|item| ({condition}).then_some(item.{}))",
                capture.element_field
            );
            output.push_str("        .collect();\n");
        }
        let _ = writeln!(output, "    let {} = {target} {{", wrapper.native_field);
    } else {
        let _ = writeln!(output, "    Ok({target} {{");
    }
    if raw_c && pair.c_chained {
        if let Some(chain) = descriptor.chains.first() {
            let local = snake_case(&chain.member);
            let _ = writeln!(output, "        nextInChain: {local}_chain,");
        } else {
            output.push_str("        nextInChain: ptr::null_mut(),\n");
        }
    }
    for member in &members {
        emit_field(
            &mut output,
            report,
            dictionary,
            member,
            raw_c,
            &string_policy,
            &unsupported,
            &skips,
            &handles,
            &handle_sequences,
            &handle_or_enum,
            &required_defaults,
            &absent_constants,
            unions,
        )?;
    }
    if raw_c {
        let zero: BTreeSet<&str> = descriptor
            .zero
            .iter()
            .flatten()
            .map(String::as_str)
            .collect();
        for member in &pair.c_only_members {
            if zero.contains(member.name.as_str()) {
                let field = rust_field_name(&member.name, true);
                let _ = writeln!(output, "        {field}: 0,");
            }
        }
        for member in embedded.keys() {
            let field = rust_field_name(member, true);
            let local = rust_field_name(member, false);
            if field == local {
                let _ = writeln!(output, "        {field},");
            } else {
                let _ = writeln!(output, "        {field}: {local},");
            }
        }
        for policy in &descriptor.union_flatten {
            for field in &policy.fields {
                let rust_field = rust_field_name(&field.c_member, true);
                let local = snake_case(&field.member);
                if rust_field == local {
                    let _ = writeln!(output, "        {rust_field},");
                } else {
                    let _ = writeln!(output, "        {rust_field}: {local},");
                }
            }
            for interface in &policy.handle_arms {
                let object = interface
                    .strip_prefix("GPU")
                    .filter(|value| !value.is_empty())
                    .unwrap_or(interface);
                let field = snake_case(object);
                let resource = format!("{field}_resource");
                let rust_field = rust_field_name(&field, true);
                let created = policy
                    .direct_handle_arms
                    .iter()
                    .find(|direct| direct.c_member == field && direct.creator_dispatch.is_some())
                    .map(|direct| format!("{}_created_resource", snake_case(&direct.c_member)));
                let value = if let Some(created) = created {
                    format!("{resource}.or({created}).unwrap_or(ptr::null_mut())")
                } else {
                    format!("{resource}.unwrap_or(ptr::null_mut())")
                };
                let _ = writeln!(output, "        {rust_field}: {value},");
            }
            for name in &policy.zero_c_members {
                let field = rust_field_name(name, true);
                let _ = writeln!(output, "        {field}: ptr::null_mut(),");
            }
        }
    }
    if let Some(wrapper) = &descriptor.wrapper {
        output.push_str("    };\n");
        let _ = writeln!(output, "    Ok({} {{", wrapper.target);
        let _ = writeln!(output, "        {},", wrapper.native_field);
        for capture in &wrapper.captures {
            let _ = writeln!(output, "        {},", capture.field);
        }
        for capture in &wrapper.sequence_captures {
            let _ = writeln!(output, "        {},", capture.field);
        }
        output.push_str("    })\n");
    } else {
        output.push_str("    })\n");
    }
    output.push_str("}\n");
    Ok(output)
}

fn emit_unsupported_check(output: &mut String, name: &str, value: &str) {
    output
        .push_str("    // G7 carve-out: fail early instead of silently emitting a wrong layout.\n");
    let _ = writeln!(output, "    if !E::is_undefined(cx, {value}) {{");
    let _ = writeln!(
        output,
        "        return Err(E::type_error(cx, \"{name} bindings are not supported yet\"));"
    );
    output.push_str("    }\n");
}

fn emit_rejected_skip_check(output: &mut String, name: &str, value: &str) {
    output.push_str("    // Policy skip: reject present unsupported API instead of ignoring it.\n");
    let _ = writeln!(output, "    if !E::is_undefined(cx, {value}) {{");
    let _ = writeln!(
        output,
        "        return Err(E::type_error(cx, \"{name} are not supported yet\"));"
    );
    output.push_str("    }\n");
}

fn emit_handle_sequence_local(
    output: &mut String,
    name: &str,
    local: &str,
    value: &str,
    helper: &str,
    required: bool,
) {
    if required {
        let _ = writeln!(output, "    let {local} = {{");
    } else {
        let _ = writeln!(
            output,
            "    let {local} = if E::is_undefined(cx, {value}) {{"
        );
        output.push_str("        &[][..]\n    } else {\n");
    }
    output
        .push_str("        // B8: conversion extracts handles only; create paths own retention.\n");
    let _ = writeln!(
        output,
        "        let converted = convert_sequence::<E, _>(cx, {value}, \"{name}\", |item| {{"
    );
    let _ = writeln!(output, "            {helper}::<E>(cx, item)");
    output.push_str("        })?;\n        arena.alloc_slice(converted)\n    };\n");
}

fn emit_handle_or_enum_local(
    output: &mut String,
    name: &str,
    local: &str,
    value: &str,
    policy: &HandleOrEnumUnionPolicy,
) {
    output.push_str(
        "    // Policy: the handle-or-enum union preserves explicit handles and auto layout.\n",
    );
    let _ = writeln!(output, "    let {local} = if E::is_null(cx, {value}) {{");
    let _ = writeln!(output, "        return Err(E::type_error(cx, \"{name}\"));");
    let _ = writeln!(
        output,
        "    }} else if let Ok(handle) = {}::<E>(cx, {value}) {{",
        policy.handle_helper
    );
    output.push_str("        handle\n    } else {\n        let union_arena = Arena::new();\n");
    let _ = writeln!(
        output,
        "        match E::to_str(cx, {value}, &union_arena)? {{"
    );
    let _ = writeln!(
        output,
        "            \"{}\" => ptr::null_mut(),",
        policy.enum_value
    );
    let _ = writeln!(
        output,
        "            _ => return Err(E::type_error(cx, \"{}\")),",
        policy.union_type
    );
    output.push_str("        }\n    };\n");
}

fn emit_union_flatten_locals(
    output: &mut String,
    report: &JoinReport,
    dictionary: &str,
    value: &str,
    policy: &UnionFlattenPolicy,
) -> Result<(), CodegenError> {
    let arm = descriptor_pair(report, &policy.arm)?;
    for interface in &policy.handle_arms {
        let object = interface
            .strip_prefix("GPU")
            .filter(|value| !value.is_empty())
            .unwrap_or(interface);
        let field = snake_case(object);
        let payload = format!("{object}Payload");
        let class = format!("GPU_{}_CLASS", field.to_ascii_uppercase());
        let resource = format!("{field}_resource");
        output.push_str(
            "    // C2/R24: wrapper-union arms are selected by generated ClassSpec identity.\n",
        );
        let _ = writeln!(
            output,
            "    let {resource} = E::payload(cx, {value}, {class})"
        );
        let _ = writeln!(
            output,
            "        .and_then(|payload| payload.downcast_ref::<{payload}>())"
        );
        let _ = writeln!(output, "        .map(|payload| payload.{field});");
    }
    for direct in &policy.direct_handle_arms {
        let object = direct
            .interface
            .strip_prefix("GPU")
            .filter(|value| !value.is_empty())
            .unwrap_or(&direct.interface);
        let base = snake_case(object);
        let payload = if direct.interface == "GPUBuffer" {
            format!("{object}Payload<E>")
        } else {
            format!("{object}Payload")
        };
        let class = format!("GPU_{}_CLASS", base.to_ascii_uppercase());
        let resource = format!("{base}_direct_resource");
        output.push_str(
            "    // B-4b: direct union arms are selected by generated ClassSpec identity.\n",
        );
        let _ = writeln!(
            output,
            "    let {resource} = E::payload(cx, {value}, {class})"
        );
        let _ = writeln!(
            output,
            "        .and_then(|payload| payload.downcast_ref::<{payload}>())"
        );
        if let Some(field) = &direct.payload_field {
            let _ = writeln!(output, "        .map(|payload| payload.{field});");
        } else {
            output.push_str("        .map(|_| ())\n");
            let helper = direct
                .handle_helper
                .as_deref()
                .expect("validated handle helper");
            let _ = writeln!(output, "        .map(|_| {helper}::<E>(cx, {value}))");
            output.push_str("        .transpose()?;\n");
        }
        if let Some(dispatch) = &direct.creator_dispatch {
            let created_resource = format!("{}_created_resource", snake_case(&direct.c_member));
            let capture = direct
                .created_capture
                .as_deref()
                .expect("validated created capture");
            let _ = writeln!(
                output,
                "    let {created_resource} = if let Some(source) = {resource} {{"
            );
            let _ = writeln!(output, "        let created = unsafe {{ (E::environment(cx).gpu().{dispatch})(source, ptr::null()) }};");
            output.push_str("        if created.is_null() {\n");
            let _ = writeln!(output, "            return Err(E::operation_error(cx, \"wgpuTextureCreateView returned null\"));");
            output.push_str("        }\n");
            let _ = writeln!(output, "        {capture}.push(created);");
            output.push_str("        Some(created)\n    } else {\n        None\n    };\n");
        }
    }
    let mut resource_checks = policy
        .handle_arms
        .iter()
        .map(|interface| {
            let object = interface
                .strip_prefix("GPU")
                .filter(|value| !value.is_empty())
                .unwrap_or(interface);
            format!("{}_resource.is_some()", snake_case(object))
        })
        .collect::<Vec<_>>();
    resource_checks.extend(policy.direct_handle_arms.iter().map(|direct| {
        let object = direct
            .interface
            .strip_prefix("GPU")
            .filter(|value| !value.is_empty())
            .unwrap_or(&direct.interface);
        format!("{}_direct_resource.is_some()", snake_case(object))
    }));
    let handle_resource = if resource_checks.is_empty() {
        "false".to_owned()
    } else {
        resource_checks.join(" || ")
    };
    for field in &policy.fields {
        let idl = idl_dictionary_value(arm, &field.member).ok_or_else(|| {
            unsupported_shape(dictionary, &field.member, "missing union arm field")
        })?;
        let local = snake_case(&field.member);
        let field_value = format!("{local}_value");
        if let Some(helper) = &field.handle_helper {
            output.push_str(
                "    // B8: flattened handle conversion extracts only the native handle.\n",
            );
            let direct_resource = policy
                .direct_handle_arms
                .iter()
                .find(|direct| {
                    direct.c_member == field.c_member && direct.creator_dispatch.is_none()
                })
                .map(|direct| {
                    let object = direct
                        .interface
                        .strip_prefix("GPU")
                        .unwrap_or(&direct.interface);
                    format!("{}_direct_resource", snake_case(object))
                });
            let _ = writeln!(
                output,
                "    let {local} = if let Some(direct) = {} {{",
                direct_resource.as_deref().unwrap_or("None")
            );
            output.push_str("        direct\n");
            let _ = writeln!(output, "    }} else if {handle_resource} {{");
            output.push_str("        ptr::null_mut()\n    } else {\n");
            let _ = writeln!(
                output,
                "        let {field_value} = E::get_property(cx, {value}, \"{}\")?;",
                field.member
            );
            let _ = writeln!(output, "        if E::is_undefined(cx, {field_value}) {{");
            let _ = writeln!(
                output,
                "            return Err(E::type_error(cx, \"{}\"));",
                policy.unsupported_error
            );
            output.push_str("        }\n");
            let _ = writeln!(output, "        {helper}::<E>(cx, {field_value})?");
            output.push_str("    };\n");
            continue;
        }
        let default = if let Some(constant) = &field.absent_constant {
            format!("{constant} as u64")
        } else {
            idl.default_value.clone().ok_or_else(|| {
                unsupported_shape(
                    dictionary,
                    &field.member,
                    "flattened integer has no default",
                )
            })?
        };
        let conversion = match idl.integer_width {
            Some(64) => format!("enforce_u64::<E>(cx, {field_value}, \"{}\")?", field.member),
            Some(32) => format!("enforce_u32::<E>(cx, {field_value}, \"{}\")?", field.member),
            _ => {
                return Err(unsupported_shape(
                    dictionary,
                    &field.member,
                    "flattened integer has unsupported width",
                ))
            }
        };
        output.push_str("    // R8: flattened `[EnforceRange]` members keep their WebIDL width.\n");
        let _ = writeln!(output, "    let {local} = if {handle_resource} {{");
        let _ = writeln!(output, "        {default}");
        output.push_str("    } else {\n");
        let _ = writeln!(
            output,
            "        let {field_value} = E::get_property(cx, {value}, \"{}\")?;",
            field.member
        );
        let _ = writeln!(output, "        if E::is_undefined(cx, {field_value}) {{");
        let _ = writeln!(output, "            {default}");
        output.push_str("        } else {\n");
        let _ = writeln!(output, "            {conversion}");
        output.push_str("        }\n");
        output.push_str("    };\n");
    }
    Ok(())
}

fn emit_chain_local(
    output: &mut String,
    name: &str,
    value: &str,
    idl: &ValueModel,
    policy: &ChainPolicy,
) {
    let local = snake_case(name);
    let target_field = rust_field_name(&policy.field, true);
    if is_idl_string(idl) {
        let _ = writeln!(output, "    let {local} = E::to_str(cx, {value}, arena)?;");
        output.push_str(
            "    // B3: WGSL is represented by an arena-owned chained struct with sType set.\n",
        );
    } else {
        let _ = writeln!(
            output,
            "    let {local}_chain = if E::is_undefined(cx, {value}) {{"
        );
        output.push_str("        ptr::null_mut()\n    } else {\n");
        let _ = writeln!(
            output,
            "        let {local} = enforce_u64::<E>(cx, {value}, \"{name}\")?;"
        );
        output.push_str(
            "        // An explicitly provided optional value is represented by an arena-owned chained struct.\n",
        );
    }
    let indent = if is_idl_string(idl) {
        "    "
    } else {
        "        "
    };
    let _ = writeln!(
        output,
        "{indent}let {local}_source = arena.alloc_slice(vec![{} {{",
        policy.target,
    );
    let _ = writeln!(output, "{indent}    chain: WGPUChainedStruct {{");
    let _ = writeln!(output, "{indent}        next: ptr::null_mut(),");
    let _ = writeln!(output, "{indent}        sType: {},", policy.s_type);
    let _ = writeln!(output, "{indent}    }},");
    if is_idl_string(idl) {
        let _ = writeln!(
            output,
            "{indent}    {target_field}: WGPUStringView::from_bytes({local}.as_bytes()),"
        );
    } else {
        let _ = writeln!(output, "{indent}    {target_field}: {local},");
    }
    let _ = writeln!(output, "{indent}}}]).as_ptr();");
    let _ = writeln!(
        output,
        "{indent}// SAFETY: the arena allocation contains one initialized chained source."
    );
    if is_idl_string(idl) {
        let _ = writeln!(
            output,
            "{indent}let {local}_chain = unsafe {{ ptr::addr_of!((*{local}_source).chain) }}.cast_mut();"
        );
    } else {
        let _ = writeln!(
            output,
            "        unsafe {{ ptr::addr_of!((*{local}_source).chain) }}.cast_mut()"
        );
        output.push_str("    };\n");
    }
}

fn emit_dict_or_sequence_union(
    report: &JoinReport,
    policy: &DictOrSequenceUnionPolicy,
    descriptors: &BTreeMap<&str, &DescriptorEntry>,
) -> Result<String, CodegenError> {
    let pair = descriptor_pair(report, &policy.dictionary)?;
    let descriptor = descriptors.get(policy.dictionary.as_str()).ok_or_else(|| {
        CodegenError::Policy(format!(
            "dict-or-sequence union {} lost selected dictionary {}",
            policy.typedef, policy.dictionary
        ))
    })?;
    if descriptor_needs_arena(pair, descriptor) {
        return Err(unsupported_shape(
            &policy.typedef,
            &policy.dictionary,
            "dict-or-sequence dictionary unexpectedly needs an arena",
        ));
    }
    let target = pair.c_name.as_deref().ok_or_else(|| {
        unsupported_shape(
            &policy.typedef,
            &policy.dictionary,
            "dictionary has no C target",
        )
    })?;
    let function = format!("convert_{}", snake_case(&policy.typedef));
    let dictionary_function = format!(
        "convert_{}",
        snake_case(
            policy
                .dictionary
                .strip_prefix("GPU")
                .unwrap_or(&policy.dictionary)
        )
    );
    let mut output = String::new();
    let _ = writeln!(
        output,
        "/// Converts the dictionary-or-sequence `{}` typedef into `{target}`.",
        policy.typedef
    );
    output.push_str("#[allow(dead_code)] // T1 policy selects both typedefs; some land before their API consumer.\n");
    let _ = writeln!(
        output,
        "pub(super) fn {function}<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<{target}, E::Error> {{"
    );
    output.push_str("    // T1: only an object can select the sequence or dictionary union arm.\n");
    let _ = writeln!(
        output,
        "    if !E::is_object(cx, value) {{ return Err(E::type_error(cx, \"{} must be an object\")); }}",
        policy.typedef
    );
    output.push_str("    // T1: an iterable object selects the sequence arm; otherwise dictionary conversion applies.\n");
    output.push_str(
        "    let Some(iterator_method) = sequence_iterator_method::<E>(cx, value)? else {\n",
    );
    let _ = writeln!(
        output,
        "        return {dictionary_function}::<E>(cx, value);"
    );
    output.push_str("    };\n");
    let _ = writeln!(
        output,
        "    let values = convert_sequence_from_method::<E, _>(cx, value, iterator_method, \"{}\", |item| {{",
        policy.typedef
    );
    let element_type = pair
        .members
        .first()
        .and_then(|member| member.idl.first())
        .and_then(|member| member.values.first())
        .map(|value| value.type_name.as_str())
        .unwrap_or_default();
    match element_type {
        "GPUIntegerCoordinate" => {
            output.push_str("        enforce_u32::<E>(cx, item, \"coordinate\")\n    })?;\n")
        }
        "double" => {
            output.push_str("        restricted_f64::<E>(cx, item, \"color channel\")\n    })?;\n")
        }
        _ => {
            return Err(unsupported_shape(
                &policy.typedef,
                &policy.dictionary,
                "unsupported sequence element type",
            ))
        }
    }
    if policy.min_length == 0 {
        let _ = writeln!(output, "    if values.len() > {} {{", policy.max_length);
    } else if policy.min_length == 1 {
        let _ = writeln!(
            output,
            "    if values.is_empty() || values.len() > {} {{",
            policy.max_length
        );
    } else {
        let _ = writeln!(
            output,
            "    if values.len() < {} || values.len() > {} {{",
            policy.min_length, policy.max_length
        );
    }
    let _ = writeln!(
        output,
        "        return Err(E::type_error(cx, \"{} sequence length must be {}..={}\"));",
        policy.typedef, policy.min_length, policy.max_length
    );
    output.push_str("    }\n");
    let _ = writeln!(output, "    Ok({target} {{");
    for (index, member) in pair.members.iter().enumerate() {
        let (idl, c) = member_values(member, &policy.dictionary)?;
        let field = rust_field_name(&c.name, true);
        if index < policy.min_length {
            let _ = writeln!(output, "        {field}: values[{index}],");
        } else {
            let default = idl.default_value.as_deref().ok_or_else(|| {
                unsupported_shape(
                    &policy.typedef,
                    &member.member,
                    "trailing field has no default",
                )
            })?;
            let accessor = if index == 0 {
                "values.first()".to_owned()
            } else {
                format!("values.get({index})")
            };
            let _ = writeln!(
                output,
                "        {field}: {accessor}.copied().unwrap_or({default}),"
            );
        }
    }
    output.push_str("    })\n}\n");
    Ok(output)
}

fn emit_string_local(
    output: &mut String,
    dictionary: &str,
    name: &str,
    local: &str,
    value: &str,
    nullable: bool,
    idl: &ValueModel,
) -> Result<(), CodegenError> {
    if nullable {
        output.push_str(
            "    // B4: nullable strings default for undefined or null as classified by policy.\n",
        );
        let _ = writeln!(
            output,
            "    let {local} = if E::is_undefined(cx, {value}) || E::is_null(cx, {value}) {{"
        );
        output.push_str("        None\n    } else {\n");
        let _ = writeln!(output, "        Some(E::to_str(cx, {value}, arena)?)");
        output.push_str("    };\n");
    } else if !idl.required && idl.default_value.is_none() {
        output.push_str(
            "    // B4: optional non-nullable strings preserve absence; present null is stringified.\n",
        );
        let _ = writeln!(
            output,
            "    let {local} = if E::is_undefined(cx, {value}) {{"
        );
        output.push_str("        None\n    } else {\n");
        let _ = writeln!(output, "        Some(E::to_str(cx, {value}, arena)?)");
        output.push_str("    };\n");
    } else {
        output.push_str(
            "    // B4: non-nullable strings default only for undefined; null is stringified.\n",
        );
        let default = idl.default_value.as_deref().ok_or_else(|| {
            unsupported_shape(
                dictionary,
                name,
                "non-nullable string without an IDL default",
            )
        })?;
        if default != "\"\"" {
            return Err(unsupported_shape(
                dictionary,
                name,
                "non-empty non-nullable string default",
            ));
        }
        let _ = writeln!(
            output,
            "    let {local} = if E::is_undefined(cx, {value}) {{"
        );
        output.push_str("        \"\"\n    } else {\n");
        let _ = writeln!(output, "        E::to_str(cx, {value}, arena)?");
        output.push_str("    };\n");
    }
    Ok(())
}

fn emit_enum_local(
    output: &mut String,
    report: &JoinReport,
    dictionary: &str,
    name: &str,
    value: &str,
    idl: &ValueModel,
    prefer_sentinel: bool,
) -> Result<(), CodegenError> {
    let local = rust_field_name(name, false);
    let pair = enum_pair(report, &idl.type_name).ok_or_else(|| {
        unsupported_shape(dictionary, name, &format!("missing enum {}", idl.type_name))
    })?;
    let c_type = pair.c_name.as_deref().ok_or_else(|| {
        unsupported_shape(
            dictionary,
            name,
            &format!("enum {} has no C type", idl.type_name),
        )
    })?;
    let undefined = pair
        .enum_values
        .iter()
        .find(|value| {
            value.idl_value.is_none()
                && value
                    .c_value
                    .as_deref()
                    .is_some_and(|value| canonical(value) == "undefined")
        })
        .and_then(|value| value.c_value.as_deref());
    let idl_default = idl
        .default_value
        .as_deref()
        .and_then(|value| value.strip_prefix('"'))
        .and_then(|value| value.strip_suffix('"'));
    let default = if idl.required {
        None
    } else if prefer_sentinel {
        let undefined = undefined.ok_or_else(|| {
            unsupported_shape(dictionary, name, "enum has no C undefined sentinel")
        })?;
        Some(enum_constant(c_type, undefined))
    } else if let Some(idl_default) = idl_default {
        let value = pair
            .enum_values
            .iter()
            .find(|value| value.idl_value.as_deref() == Some(idl_default))
            .and_then(|value| value.c_value.as_deref())
            .ok_or_else(|| unsupported_shape(dictionary, name, "enum default is not joined"))?;
        Some(enum_constant(c_type, value))
    } else if let Some(undefined) = undefined {
        Some(enum_constant(c_type, undefined))
    } else {
        return Err(unsupported_shape(
            dictionary,
            name,
            "enum has no C undefined sentinel or IDL default",
        ));
    };
    output.push_str(
        "    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.\n",
    );
    if let Some(default) = default {
        let _ = writeln!(
            output,
            "    let {local} = if E::is_undefined(cx, {value}) {{"
        );
        let _ = writeln!(output, "        {default}");
        output.push_str("    } else {\n");
    } else {
        let _ = writeln!(output, "    let {local} = {{");
    }
    output.push_str("        let enum_arena = Arena::new();\n");
    let _ = writeln!(
        output,
        "        match E::to_str(cx, {value}, &enum_arena)? {{"
    );
    for enum_value in &pair.enum_values {
        if let (Some(idl_value), Some(c_value)) = (&enum_value.idl_value, &enum_value.c_value) {
            let constant = enum_constant(c_type, c_value);
            let _ = writeln!(output, "            \"{idl_value}\" => {constant},");
        }
    }
    let _ = writeln!(
        output,
        "            _ => return Err(E::type_error(cx, \"{}\")),",
        idl.type_name
    );
    output.push_str("        }\n    };\n");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_nested_local(
    output: &mut String,
    report: &JoinReport,
    dictionary: &str,
    name: &str,
    local: &str,
    value: &str,
    idl: &ValueModel,
    c: &ValueModel,
    descriptors: &BTreeMap<&str, &DescriptorEntry>,
    outer_created_capture: Option<&str>,
) -> Result<(), CodegenError> {
    let nested = descriptors.get(idl.type_name.as_str()).ok_or_else(|| {
        unsupported_shape(
            dictionary,
            name,
            &format!("nested dictionary {} is not selected", idl.type_name),
        )
    })?;
    let optional_pointer = !idl.required && c.pointer.as_deref() == Some("immutable");
    let default_dictionary = !idl.required && idl.default_value.as_deref() == Some("{}");
    if !idl.required
        && !optional_pointer
        && !default_dictionary
        && c.default_value.as_deref() != Some("zero")
    {
        return Err(unsupported_shape(
            dictionary,
            name,
            "optional nested dictionary does not have a zero C default",
        ));
    }
    let nested_pair = descriptor_pair(report, &idl.type_name)?;
    let nested_function = format!(
        "convert_{}",
        snake_case(idl.type_name.strip_prefix("GPU").unwrap_or(&idl.type_name))
    );
    let nested_needs_arena = descriptor_needs_arena(nested_pair, nested);
    let capture_arg = nested
        .created_view_capture
        .as_deref()
        .map(|capture| {
            assert_eq!(outer_created_capture, Some(capture));
            format!(", {capture}")
        })
        .unwrap_or_default();
    if idl.required || default_dictionary {
        if nested_needs_arena {
            let _ = writeln!(
                output,
                "    let {local} = {nested_function}::<E>(cx, {value}, arena{capture_arg})?;"
            );
        } else {
            let _ = writeln!(
                output,
                "    let {local} = {nested_function}::<E>(cx, {value}{capture_arg})?;"
            );
        }
        return Ok(());
    }
    if optional_pointer {
        output.push_str(
            "    // T5: an absent optional dictionary is a null pointer in the pinned C ABI.\n",
        );
        let _ = writeln!(
            output,
            "    let {local} = if E::is_undefined(cx, {value}) {{"
        );
        output.push_str("        ptr::null()\n    } else {\n");
        if nested_needs_arena {
            let _ = writeln!(
                output,
                "        let converted = {nested_function}::<E>(cx, {value}, arena{capture_arg})?;"
            );
        } else {
            let _ = writeln!(
                output,
                "        let converted = {nested_function}::<E>(cx, {value}{capture_arg})?;"
            );
        }
        output.push_str("        arena.alloc_slice(vec![converted]).as_ptr()\n    };\n");
        return Ok(());
    }
    output.push_str(
        "    // G11: an absent nested dictionary preserves the C zero/default sentinel.\n",
    );
    let _ = writeln!(
        output,
        "    let {local} = if E::is_undefined(cx, {value}) {{"
    );
    output.push_str("        // SAFETY: the joined C-ABI member declares `default: zero`.\n");
    output.push_str("        unsafe { std::mem::zeroed() }\n    } else {\n");
    if nested_needs_arena {
        let _ = writeln!(
            output,
            "        {nested_function}::<E>(cx, {value}, arena{capture_arg})?"
        );
    } else {
        let _ = writeln!(
            output,
            "        {nested_function}::<E>(cx, {value}{capture_arg})?"
        );
    }
    output.push_str("    };\n");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_sequence_local(
    output: &mut String,
    report: &JoinReport,
    dictionary: &str,
    name: &str,
    local: &str,
    value: &str,
    element: &str,
    default_empty: bool,
    descriptors: &BTreeMap<&str, &DescriptorEntry>,
    outer_created_capture: Option<&str>,
) -> Result<(), CodegenError> {
    let element_nullable = element.ends_with('?');
    let element = element.trim_end_matches('?');
    let mut sequence_created_captures: BTreeSet<_> = descriptors
        .get(element)
        .into_iter()
        .flat_map(|nested| nested.union_flatten.iter())
        .flat_map(|flatten| flatten.direct_handle_arms.iter())
        .filter_map(|arm| arm.created_capture.as_deref())
        .collect();
    if let Some(capture) = descriptors
        .get(element)
        .and_then(|nested| nested.created_view_capture.as_deref())
    {
        sequence_created_captures.insert(capture);
    }
    for capture in &sequence_created_captures {
        if outer_created_capture != Some(*capture) {
            let _ = writeln!(
                output,
                "    let mut {capture} = CreatedTextureViewCapture::new::<E>(cx);"
            );
        }
    }
    let _ = writeln!(
        output,
        "    let {local} = {}",
        if default_empty {
            "if E::is_undefined(cx, ".to_owned() + value + ") {\n        &[][..]\n    } else {"
        } else {
            "{".to_owned()
        }
    );
    let _ = writeln!(
        output,
        "        let converted = convert_sequence::<E, _>(cx, {value}, \"{name}\", |item| {{"
    );
    if is_dictionary(report, element) {
        let nested = descriptors.get(element).ok_or_else(|| {
            unsupported_shape(
                dictionary,
                name,
                &format!("sequence dictionary {element} is not selected"),
            )
        })?;
        let nested_pair = descriptor_pair(report, element)?;
        let nested_function = format!(
            "convert_{}",
            snake_case(element.strip_prefix("GPU").unwrap_or(element))
        );
        if element_nullable {
            output.push_str("            // T5: nullable sequence elements are C sentinel-filled struct holes.\n");
            output.push_str("            if E::is_null(cx, item) || E::is_undefined(cx, item) {\n");
            if element == "GPURenderPassColorAttachment" {
                output.push_str("                // The pinned webgpu.h INIT macro defines a hole with a null view,\n");
                output.push_str("                // undefined depth slice/load/store values, and a zero color.\n");
                output.push_str("                Ok(WGPURenderPassColorAttachment {\n");
                output.push_str("                    nextInChain: ptr::null_mut(),\n");
                output.push_str("                    view: ptr::null_mut(),\n");
                output.push_str("                    depthSlice: WGPU_DEPTH_SLICE_UNDEFINED,\n");
                output.push_str("                    resolveTarget: ptr::null_mut(),\n");
                output.push_str("                    loadOp: WGPULoadOp_WGPULoadOp_Undefined,\n");
                output
                    .push_str("                    storeOp: WGPUStoreOp_WGPUStoreOp_Undefined,\n");
                output.push_str(
                    "                    // SAFETY: WGPU_COLOR_INIT is the all-zero WGPUColor.\n",
                );
                output.push_str("                    clearValue: unsafe { std::mem::zeroed() },\n");
                output.push_str("                })\n");
            } else {
                output.push_str("                // SAFETY: the pinned C ABI defines the all-zero element as the hole sentinel.\n");
                output.push_str("                Ok(unsafe { std::mem::zeroed() })\n");
            }
            output.push_str("            } else {\n");
        }
        if descriptor_needs_arena(nested_pair, nested) {
            let extra = sequence_created_captures
                .iter()
                .map(|capture| {
                    if outer_created_capture == Some(*capture) {
                        format!(", {capture}")
                    } else {
                        format!(", &mut {capture}")
                    }
                })
                .collect::<String>();
            let _ = writeln!(
                output,
                "{} {nested_function}::<E>(cx, item, arena{extra})",
                if element_nullable {
                    "               "
                } else {
                    "           "
                }
            );
        } else {
            let extra = sequence_created_captures
                .iter()
                .map(|capture| {
                    if outer_created_capture == Some(*capture) {
                        format!(", {capture}")
                    } else {
                        format!(", &mut {capture}")
                    }
                })
                .collect::<String>();
            let _ = writeln!(
                output,
                "{} {nested_function}::<E>(cx, item{extra})",
                if element_nullable {
                    "               "
                } else {
                    "           "
                }
            );
        }
        if element_nullable {
            output.push_str("            }\n");
        }
    } else if let Some(pair) = enum_pair(report, element) {
        let c_type = pair.c_name.as_deref().ok_or_else(|| {
            unsupported_shape(dictionary, name, &format!("enum {element} has no C type"))
        })?;
        if element_nullable {
            output.push_str("            // Nullable enum elements use the C enum's Undefined sentinel as holes.\n");
            output.push_str("            if E::is_null(cx, item) || E::is_undefined(cx, item) {\n");
            let undefined = enum_constant(c_type, "undefined");
            let _ = writeln!(output, "                Ok({undefined})");
            output.push_str("            } else {\n");
        }
        output.push_str("            let enum_arena = Arena::new();\n");
        output.push_str("            match E::to_str(cx, item, &enum_arena)? {\n");
        for enum_value in &pair.enum_values {
            if let (Some(idl_value), Some(c_value)) = (&enum_value.idl_value, &enum_value.c_value) {
                let constant = enum_constant(c_type, c_value);
                let _ = writeln!(output, "                \"{idl_value}\" => Ok({constant}),");
            }
        }
        let _ = writeln!(
            output,
            "                _ => Err(E::type_error(cx, \"{element}\")),"
        );
        output.push_str("            }\n");
        if element_nullable {
            output.push_str("            }\n");
        }
    } else {
        return Err(unsupported_shape(
            dictionary,
            name,
            &format!("sequence element {element} is neither a dictionary nor an enum"),
        ));
    }
    output.push_str("        })?;\n        arena.alloc_slice(converted)\n    };\n");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_field(
    output: &mut String,
    report: &JoinReport,
    dictionary: &str,
    member: &MemberPair,
    raw_c: bool,
    string_policy: &BTreeMap<&str, bool>,
    unsupported: &BTreeSet<&str>,
    skips: &BTreeMap<&str, &SkipPolicy>,
    handles: &BTreeMap<&str, &crate::HandlePolicy>,
    handle_sequences: &BTreeMap<&str, &str>,
    handle_or_enum: &BTreeMap<&str, &HandleOrEnumUnionPolicy>,
    required_defaults: &BTreeMap<&str, u64>,
    absent_constants: &BTreeMap<&str, &str>,
    unions: &BTreeMap<&str, &DictOrSequenceUnionPolicy>,
) -> Result<(), CodegenError> {
    let (idl, c) = member_values(member, dictionary)?;
    let name = &member.member;
    let field = rust_field_name(&c.name, raw_c);
    let local = rust_field_name(name, false);
    let value = format!("{}_value", snake_case(name));
    if dictionary == "GPURenderPassColorAttachment" && name == "depthSlice" {
        let _ = writeln!(
            output,
            "        {field}: depth_slice.unwrap_or(WGPU_DEPTH_SLICE_UNDEFINED),"
        );
        return Ok(());
    }
    if unsupported.contains(name.as_str()) {
        output.push_str(
            "        // SAFETY: policy permits only a joined `default: zero` C member here.\n",
        );
        let _ = writeln!(output, "        {field}: unsafe {{ std::mem::zeroed() }},");
        return Ok(());
    }
    if let Some(skip) = skips.get(name.as_str()) {
        let _ = writeln!(output, "        // Policy skip: {}.", skip.reason);
        if c.count_and_pointer {
            let count = count_field_name(&c.name);
            let _ = writeln!(output, "        {count}: 0,");
            let _ = writeln!(output, "        {field}: ptr::null(),");
        } else if c.pointer.as_deref() == Some("immutable") {
            let _ = writeln!(output, "        {field}: ptr::null(),");
        } else if c.pointer.is_some() || c.type_name.starts_with("WGPU") {
            let _ = writeln!(output, "        {field}: ptr::null_mut(),");
        } else {
            let _ = writeln!(output, "        {field}: 0,");
        }
        return Ok(());
    }
    if handles.contains_key(name.as_str()) || handle_or_enum.contains_key(name.as_str()) {
        if field == local {
            let _ = writeln!(output, "        {field},");
        } else {
            let _ = writeln!(output, "        {field}: {local},");
        }
        return Ok(());
    }
    if handle_sequences.contains_key(name.as_str()) && c.count_and_pointer {
        let count = count_field_name(&c.name);
        let _ = writeln!(output, "        {count}: {local}.len(),");
        let _ = writeln!(output, "        {field}: if {local}.is_empty() {{");
        output.push_str("            ptr::null()\n        } else {\n");
        let _ = writeln!(output, "            {local}.as_ptr()");
        output.push_str("        },\n");
        return Ok(());
    }
    if is_idl_string(idl) && c.string_view {
        if raw_c {
            if string_policy[name.as_str()] || (!idl.required && idl.default_value.is_none()) {
                let _ = writeln!(output, "        {field}: {local}.map_or_else(");
                output.push_str(
                    "            || WGPUStringView { data: ptr::null(), length: wgpu_strlen() },\n",
                );
                output.push_str("            |value| WGPUStringView::from_bytes(value.as_bytes()),\n        ),\n");
            } else {
                let _ = writeln!(
                    output,
                    "        {field}: WGPUStringView::from_bytes({local}.as_bytes()),"
                );
            }
        } else if string_policy[name.as_str()] || (!idl.required && idl.default_value.is_none()) {
            let _ = writeln!(output, "        {field}: {local}.map(str::to_owned),");
        } else {
            let _ = writeln!(output, "        {field}: {local}.to_owned(),");
        }
        return Ok(());
    }
    if is_enum(report, &idl.type_name)
        || is_dictionary(report, &idl.type_name)
        || unions.contains_key(idl.type_name.as_str())
    {
        if field == local {
            let _ = writeln!(output, "        {field},");
        } else {
            let _ = writeln!(output, "        {field}: {local},");
        }
        return Ok(());
    }
    if is_sequence(&idl.type_name) && c.count_and_pointer {
        let count = count_field_name(&c.name);
        let _ = writeln!(output, "        {count}: {local}.len(),");
        let _ = writeln!(output, "        {field}: if {local}.is_empty() {{");
        output.push_str("            ptr::null()\n        } else {\n");
        let _ = writeln!(output, "            {local}.as_ptr()");
        output.push_str("        },\n");
        return Ok(());
    }
    if is_string_double_record(idl, c) {
        let count = count_field_name(&c.name);
        let _ = writeln!(output, "        {count}: {local}.len(),");
        let _ = writeln!(output, "        {field}: if {local}.is_empty() {{");
        output.push_str("            ptr::null()\n        } else {\n");
        let _ = writeln!(output, "            {local}.as_ptr()");
        output.push_str("        },\n");
        return Ok(());
    }
    if idl.type_name == "boolean" && c.type_name == "WGPUOptionalBool" {
        if idl.required || idl.default_value.is_some() {
            return Err(unsupported_shape(
                dictionary,
                name,
                "optional-bool C sentinel requires an optional WebIDL boolean without a default",
            ));
        }
        output.push_str(
            "        // T5: an omitted optional boolean maps to WGPUOptionalBool_Undefined.\n",
        );
        let _ = writeln!(
            output,
            "        {field}: if E::is_undefined(cx, {value}) {{"
        );
        output.push_str("            WGPUOptionalBool_WGPUOptionalBool_Undefined\n");
        output.push_str("        } else if E::to_bool(cx, ");
        output.push_str(&value);
        output.push_str(") {\n            WGPUOptionalBool_WGPUOptionalBool_True\n");
        output.push_str(
            "        } else {\n            WGPUOptionalBool_WGPUOptionalBool_False\n        },\n",
        );
        return Ok(());
    }
    if idl.type_name == "boolean" && c.type_name == "WGPUBool" {
        output.push_str("        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.\n");
        if idl.required {
            if raw_c {
                let _ = writeln!(
                    output,
                    "        {field}: u32::from(E::to_bool(cx, {value})),"
                );
            } else {
                let _ = writeln!(output, "        {field}: E::to_bool(cx, {value}),");
            }
        } else {
            let default = idl.default_value.as_deref().ok_or_else(|| {
                unsupported_shape(dictionary, name, "optional boolean without an IDL default")
            })?;
            if default != "false" {
                return Err(unsupported_shape(
                    dictionary,
                    name,
                    "boolean default other than false",
                ));
            }
            let _ = writeln!(
                output,
                "        {field}: if E::is_undefined(cx, {value}) {{"
            );
            let _ = writeln!(output, "            {}", if raw_c { "0" } else { "false" });
            output.push_str("        } else {\n");
            if raw_c {
                let _ = writeln!(output, "            u32::from(E::to_bool(cx, {value}))");
            } else {
                let _ = writeln!(output, "            E::to_bool(cx, {value})");
            }
            output.push_str("        },\n");
        }
        return Ok(());
    }
    if idl.clamp {
        output
            .push_str("        // WebIDL `[Clamp]`: NaN becomes +0, the value is clamped to the\n");
        output.push_str(
            "        // unsigned-short range, then rounded to the nearest integer (ties to even).\n",
        );
        let conversion = format!("clamp_u16::<E>(cx, {value})?");
        if idl.required {
            let _ = writeln!(output, "        {field}: {conversion},");
        } else {
            let default = idl.default_value.as_deref().ok_or_else(|| {
                unsupported_shape(
                    dictionary,
                    name,
                    "optional Clamp integer without an IDL default",
                )
            })?;
            let _ = writeln!(
                output,
                "        {field}: if E::is_undefined(cx, {value}) {{"
            );
            let _ = writeln!(output, "            {default}");
            output.push_str("        } else {\n");
            let _ = writeln!(output, "            {conversion}");
            output.push_str("        },\n");
        }
        return Ok(());
    }
    if idl.enforce_range {
        let conversion = match (idl.integer_width, c.integer_width, c.type_name.as_str()) {
            (Some(32), Some(32), "int32_t") => {
                output.push_str(
                    "        // T5: signed `[EnforceRange]` long is checked at the i32 boundary.\n",
                );
                format!("enforce_i32::<E>(cx, {value}, \"{name}\")?")
            }
            (Some(64), Some(64), _) => {
                let _ = writeln!(
                    output,
                    "        // R8: `[EnforceRange]` {} is checked at the 64-bit boundary.",
                    idl.type_name
                );
                format!("enforce_u64::<E>(cx, {value}, \"{name}\")?")
            }
            (Some(32), Some(64), _) => {
                output.push_str(
                    "        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.\n",
                );
                format!("u64::from(enforce_u32::<E>(cx, {value}, \"{name}\")?)")
            }
            (Some(32), Some(32), _) => {
                let _ = writeln!(
                    output,
                    "        // R8: `[EnforceRange]` {} is checked at the 32-bit boundary.",
                    idl.type_name
                );
                format!("enforce_u32::<E>(cx, {value}, \"{name}\")?")
            }
            _ => {
                return Err(unsupported_shape(
                    dictionary,
                    name,
                    "unsupported integer widths",
                ))
            }
        };
        if let Some(default) = required_defaults.get(name.as_str()) {
            let _ = writeln!(
                output,
                "        {field}: if E::is_undefined(cx, {value}) {{"
            );
            let _ = writeln!(output, "            {default}");
            output.push_str("        } else {\n");
            let _ = writeln!(output, "            {conversion}");
            output.push_str("        },\n");
        } else if idl.required {
            let _ = writeln!(output, "        {field}: {conversion},");
        } else {
            let default = idl
                .default_value
                .as_deref()
                .or_else(|| absent_constants.get(name.as_str()).copied())
                .ok_or_else(|| {
                    unsupported_shape(dictionary, name, "optional integer without a default")
                })?;
            let _ = writeln!(
                output,
                "        {field}: if E::is_undefined(cx, {value}) {{"
            );
            let _ = writeln!(output, "            {default}");
            output.push_str("        } else {\n");
            let _ = writeln!(output, "            {conversion}");
            output.push_str("        },\n");
        }
        return Ok(());
    }
    if idl.type_name == "float" && c.type_name == "float" {
        output.push_str(
            "        // G11: restricted WebIDL `float` rejects non-finite values before f32 conversion.\n",
        );
        let conversion = format!("restricted_f32::<E>(cx, {value}, \"{name}\")?");
        if idl.required {
            let _ = writeln!(output, "        {field}: {conversion},");
        } else {
            let default = idl
                .default_value
                .as_deref()
                .or_else(|| absent_constants.get(name.as_str()).copied())
                .ok_or_else(|| {
                    unsupported_shape(dictionary, name, "optional float without an IDL default")
                })?;
            let _ = writeln!(
                output,
                "        {field}: if E::is_undefined(cx, {value}) {{"
            );
            let suffix = if idl.default_value.is_some() {
                "_f32"
            } else {
                ""
            };
            let _ = writeln!(output, "            {default}{suffix}");
            output.push_str("        } else {\n");
            let _ = writeln!(output, "            {conversion}");
            output.push_str("        },\n");
        }
        return Ok(());
    }
    if idl.type_name == "double" && c.type_name == "double" {
        output.push_str("        // WebIDL restricted `double` rejects non-finite values.\n");
        let conversion = format!("restricted_f64::<E>(cx, {value}, \"{name}\")?");
        if idl.required {
            let _ = writeln!(output, "        {field}: {conversion},");
        } else {
            let default = idl.default_value.as_deref().ok_or_else(|| {
                unsupported_shape(dictionary, name, "optional double without an IDL default")
            })?;
            let _ = writeln!(
                output,
                "        {field}: if E::is_undefined(cx, {value}) {{"
            );
            let _ = writeln!(output, "            {default}_f64");
            output.push_str("        } else {\n");
            let _ = writeln!(output, "            {conversion}");
            output.push_str("        },\n");
        }
        return Ok(());
    }
    Err(unsupported_shape(
        dictionary,
        name,
        &format!(
            "shape IDL={} C-ABI={} default={:?}",
            idl.type_name, c.type_name, idl.default_value
        ),
    ))
}

fn descriptor_needs_arena(pair: &TypePair, descriptor: &DescriptorEntry) -> bool {
    let unsupported: BTreeSet<&str> = descriptor.unsupported.iter().map(String::as_str).collect();
    let skips: BTreeSet<&str> = descriptor
        .skips
        .iter()
        .map(|entry| entry.member.as_str())
        .collect();
    pair.members.iter().any(|member| {
        let idl = &member.idl[0].values[0];
        !unsupported.contains(member.member.as_str())
            && !skips.contains(member.member.as_str())
            && (is_idl_string(idl)
                || is_sequence(&idl.type_name)
                || is_string_double_record(idl, &member.c.values[0])
                || (member.c.values[0].pointer.is_some() && !member.c.values[0].count_and_pointer))
    }) || !descriptor.chains.is_empty()
        || !descriptor.handle_sequences.is_empty()
}

fn is_string_double_record(idl: &ValueModel, c: &ValueModel) -> bool {
    idl.type_name == "record<…, GPUPipelineConstantValue>"
        && c.type_name == "WGPUConstantEntry"
        && c.count_and_pointer
}

fn emit_string_double_record_local(output: &mut String, name: &str, local: &str, value: &str) {
    let _ = writeln!(
        output,
        "    let {local} = if E::is_undefined(cx, {value}) {{"
    );
    output.push_str("        &[][..]\n    } else {\n");
    let _ = writeln!(
        output,
        "        let names = E::own_property_names(cx, {value})?;"
    );
    let _ = writeln!(
        output,
        "        let mut converted = Vec::with_capacity(names.len());"
    );
    output.push_str("        for key in names {\n");
    let _ = writeln!(
        output,
        "            let item = E::get_property(cx, {value}, &key)?;"
    );
    let _ = writeln!(
        output,
        "            let value = restricted_f64::<E>(cx, item, \"{name}\")?;"
    );
    output.push_str("            let key = arena.alloc_str(&key);\n");
    output.push_str("            converted.push(WGPUConstantEntry {\n");
    output.push_str("                nextInChain: ptr::null_mut(),\n");
    output.push_str("                key: WGPUStringView::from_bytes(key.as_bytes()),\n");
    output.push_str("                value,\n");
    output.push_str("            });\n");
    output.push_str("        }\n");
    output.push_str("        arena.alloc_slice(converted)\n");
    output.push_str("    };\n");
}

fn descriptor_needs_static(
    report: &JoinReport,
    pair: &TypePair,
    descriptor: &DescriptorEntry,
    descriptors: &BTreeMap<&str, &DescriptorEntry>,
) -> bool {
    if !descriptor.handles.is_empty()
        || !descriptor.handle_sequences.is_empty()
        || !descriptor.union_flatten.is_empty()
        || !descriptor.handle_or_enum_unions.is_empty()
    {
        return true;
    }
    pair.members.iter().any(|member| {
        let idl = &member.idl[0].values[0];
        let nested_name = if is_dictionary(report, &idl.type_name) {
            Some(idl.type_name.trim_end_matches('?'))
        } else {
            sequence_element(&idl.type_name).map(|name| name.trim_end_matches('?'))
        };
        nested_name.is_some_and(|name| {
            descriptors.get(name).is_some_and(|nested| {
                descriptor_pair(report, name).is_ok_and(|nested_pair| {
                    descriptor_needs_static(report, nested_pair, nested, descriptors)
                })
            })
        })
    })
}

fn member_values<'a>(
    member: &'a MemberPair,
    dictionary: &str,
) -> Result<(&'a ValueModel, &'a ValueModel), CodegenError> {
    let idl = member
        .idl
        .first()
        .and_then(|model| model.values.first())
        .ok_or_else(|| unsupported_shape(dictionary, &member.member, "missing IDL value"))?;
    if member.idl.len() != 1 || member.idl[0].values.len() != 1 || member.c.values.len() != 1 {
        return Err(unsupported_shape(
            dictionary,
            &member.member,
            "descriptor member is not a one-to-one field",
        ));
    }
    Ok((idl, &member.c.values[0]))
}

fn idl_dictionary_value<'a>(pair: &'a TypePair, member: &str) -> Option<&'a ValueModel> {
    pair.members
        .iter()
        .find(|candidate| candidate.member == member)
        .and_then(|candidate| candidate.idl.first())
        .and_then(|candidate| candidate.values.first())
        .or_else(|| {
            pair.idl_only_members
                .iter()
                .find(|candidate| candidate.name == member)
                .and_then(|candidate| candidate.values.first())
        })
}

fn is_idl_string(value: &ValueModel) -> bool {
    matches!(
        value.type_name.trim_end_matches('?'),
        "USVString" | "DOMString" | "ByteString"
    )
}

fn is_dictionary(report: &JoinReport, type_name: &str) -> bool {
    report
        .dictionaries
        .iter()
        .any(|pair| pair.idl_name.as_deref() == Some(type_name.trim_end_matches('?')))
}

fn is_enum(report: &JoinReport, type_name: &str) -> bool {
    enum_pair(report, type_name).is_some()
}

fn enum_pair<'a>(report: &'a JoinReport, type_name: &str) -> Option<&'a TypePair> {
    report.enums.iter().find(|pair| {
        pair.idl_name.as_deref() == Some(type_name.trim_end_matches('?'))
            && !pair.enum_values.is_empty()
    })
}

fn is_sequence(type_name: &str) -> bool {
    sequence_element(type_name).is_some()
}

fn sequence_element(type_name: &str) -> Option<&str> {
    type_name
        .trim_end_matches('?')
        .strip_prefix("sequence<")
        .and_then(|value| value.strip_suffix('>'))
}

pub(crate) fn enum_constant(c_type: &str, c_value: &str) -> String {
    format!("{c_type}_{c_type}_{}", pascal_case(c_value))
}

fn rust_field_name(value: &str, raw_c: bool) -> String {
    if !raw_c {
        let mut field = snake_case(value);
        if field == "type" {
            field.push('_');
        }
        return field;
    }
    let mut parts = value.split('_');
    let mut output = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        output.push_str(&pascal_case(part));
    }
    if output == "type" {
        output.push('_');
    }
    output
}

fn count_field_name(value: &str) -> String {
    let camel = rust_field_name(value, true);
    let singular = if let Some(prefix) = camel.strip_suffix("ies") {
        format!("{prefix}y")
    } else if let Some(prefix) = camel.strip_suffix('s') {
        prefix.to_owned()
    } else {
        camel
    };
    format!("{singular}Count")
}

fn unsupported_shape(dictionary: &str, member: &str, shape: &str) -> CodegenError {
    CodegenError::Policy(format!(
        "unsupported emitted member {dictionary}.{member}: {shape}"
    ))
}

fn canonical(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn pascal_case(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            if part.is_empty() {
                return "_".to_owned();
            }
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn snake_case(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut output = String::new();
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_ascii_uppercase() {
            let previous_lower = index > 0 && chars[index - 1].is_ascii_lowercase();
            let next_lower = chars
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_lowercase());
            if index > 0 && (previous_lower || next_lower) {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
        } else {
            output.push(ch);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CMemberModel, IdlMemberKind, IdlMemberModel};

    fn value(name: &str, type_name: &str) -> ValueModel {
        ValueModel {
            name: name.to_owned(),
            type_name: type_name.to_owned(),
            ..ValueModel::default()
        }
    }

    #[test]
    fn public_emit_conversions_emits_a_model_driven_numeric_field() {
        let mut idl = value("size", "unsigned long long");
        idl.required = true;
        idl.enforce_range = true;
        idl.integer_width = Some(64);
        let mut c = value("size", "uint64_t");
        c.integer_width = Some(64);
        let report = JoinReport {
            dictionaries: vec![TypePair {
                idl_name: Some("GPUExampleDescriptor".to_owned()),
                c_name: Some("WGPUExampleDescriptor".to_owned()),
                c_chained: false,
                members: vec![MemberPair {
                    owner: "GPUExampleDescriptor".to_owned(),
                    member: "size".to_owned(),
                    idl: vec![IdlMemberModel {
                        name: "size".to_owned(),
                        kind: IdlMemberKind::DictionaryField,
                        values: vec![idl],
                        same_object: false,
                    }],
                    c: CMemberModel {
                        name: "size".to_owned(),
                        values: vec![c],
                        callback: None,
                    },
                }],
                idl_only_members: Vec::new(),
                c_only_members: Vec::new(),
                enum_values: Vec::new(),
            }],
            ..JoinReport::default()
        };
        let policy = r#"
            schema_version = 1
            subset = []
            [[descriptor]]
            dictionary = "GPUExampleDescriptor"
        "#;
        let emitted = emit_conversions(&report, policy).expect("emission");
        assert!(emitted.contains("fn convert_example_descriptor<E: JsEngine>"));
        assert!(emitted.contains("enforce_u64::<E>"));
        assert!(emitted.contains("// R8:"));
    }

    #[test]
    fn public_emit_conversions_rejects_dead_descriptor_policy() {
        let error = emit_conversions(
            &JoinReport::default(),
            "schema_version = 1\nsubset = []\n[[descriptor]]\ndictionary = \"GPUDead\"",
        )
        .expect_err("dead policy");
        assert!(
            matches!(error, CodegenError::Policy(message) if message.contains("dead descriptor"))
        );
    }

    #[test]
    fn required_handle_or_enum_only_descriptor_has_no_unused_arena_parameter() {
        let mut idl = value("layout", "(GPUPipelineLayout or GPUAutoLayoutMode)");
        idl.required = true;
        let c = value("layout", "WGPUPipelineLayout");
        let report = JoinReport {
            dictionaries: vec![TypePair {
                idl_name: Some("GPUExampleDescriptor".to_owned()),
                c_name: Some("WGPUExampleDescriptor".to_owned()),
                c_chained: false,
                members: vec![MemberPair {
                    owner: "GPUExampleDescriptor".to_owned(),
                    member: "layout".to_owned(),
                    idl: vec![IdlMemberModel {
                        name: "layout".to_owned(),
                        kind: IdlMemberKind::DictionaryField,
                        values: vec![idl],
                        same_object: false,
                    }],
                    c: CMemberModel {
                        name: "layout".to_owned(),
                        values: vec![c],
                        callback: None,
                    },
                }],
                idl_only_members: Vec::new(),
                c_only_members: Vec::new(),
                enum_values: Vec::new(),
            }],
            enums: vec![TypePair {
                idl_name: Some("GPUAutoLayoutMode".to_owned()),
                c_name: Some("WGPUAutoLayoutMode".to_owned()),
                c_chained: false,
                members: Vec::new(),
                idl_only_members: Vec::new(),
                c_only_members: Vec::new(),
                enum_values: vec![crate::EnumValuePair {
                    idl_value: Some("auto".to_owned()),
                    c_value: Some("auto".to_owned()),
                }],
            }],
            ..JoinReport::default()
        };
        let policy = r#"
            schema_version = 1
            subset = []
            [[descriptor]]
            dictionary = "GPUExampleDescriptor"
            [[descriptor.handle_or_enum_unions]]
            member = "layout"
            union_type = "(GPUPipelineLayout or GPUAutoLayoutMode)"
            handle_type = "GPUPipelineLayout"
            handle_helper = "pipeline_layout_handle"
            enum_type = "GPUAutoLayoutMode"
            enum_value = "auto"
            reason = "fixture union"
        "#;

        let emitted = emit_conversions(&report, policy).expect("handle-or-enum emission");
        assert!(emitted.contains("value: E::Value,\n) -> Result<WGPUExampleDescriptor, E::Error>"));
        assert!(emitted.contains("required_member::<E>(cx, value, \"layout\")?"));
        assert!(emitted.contains("if E::is_null(cx, layout_value)"));
        assert!(!emitted.contains("E::is_undefined(cx, layout_value)"));
    }
}
