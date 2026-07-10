//! Deterministic Rust emission for policy-selected descriptor conversions.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::{
    ChainPolicy, CodegenError, DescriptorEntry, HandleOrEnumUnionPolicy, JoinReport, MemberPair,
    Policy, TypePair, UnionFlattenPolicy, ValueModel,
};

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
    let mut output = String::new();
    for (index, descriptor) in policy.descriptor.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        let pair = descriptor_pair(report, &descriptor.dictionary)?;
        output.push_str(&emit_descriptor(report, pair, descriptor, &descriptors)?);
    }
    Ok(output)
}

pub(crate) fn validate_policy(report: &JoinReport, policy: &Policy) -> Result<(), CodegenError> {
    let selected: BTreeSet<&str> = policy
        .descriptor
        .iter()
        .map(|entry| entry.dictionary.as_str())
        .collect();
    for descriptor in &policy.descriptor {
        let pair = descriptor_pair(report, &descriptor.dictionary)?;
        validate_descriptor_policy(report, pair, descriptor, &selected)?;
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

fn validate_descriptor_policy(
    report: &JoinReport,
    pair: &TypePair,
    descriptor: &DescriptorEntry,
    selected: &BTreeSet<&str>,
) -> Result<(), CodegenError> {
    if pair.c_name.is_none() && descriptor.target.is_none() {
        return Err(CodegenError::Policy(format!(
            "descriptor {} has no joined C-ABI type or target override",
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
        descriptor.zero.iter().map(String::as_str),
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
            if !idl.type_name.trim_end_matches('?').starts_with("GPU")
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

    let flattened_c: BTreeSet<&str> = descriptor
        .union_flatten
        .iter()
        .flat_map(|entry| {
            entry
                .fields
                .iter()
                .map(|field| field.c_member.as_str())
                .chain(entry.zero_c_members.iter().map(String::as_str))
        })
        .collect();

    for member in &pair.c_only_members {
        if !zero.contains(member.name.as_str()) && !flattened_c.contains(member.name.as_str()) {
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
    if !is_idl_string(idl) || !idl.required {
        return Err(CodegenError::Policy(format!(
            "dead chain policy {}.{}: source is not a required string",
            descriptor.dictionary, chain.member
        )));
    }
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
    if !field.string_view {
        return Err(CodegenError::Policy(format!(
            "dead chain policy {}.{}: target field is not a string view",
            descriptor.dictionary, chain.member
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
    for field in &policy.fields {
        if !claimed_idl.insert(field.member.as_str()) || !claimed_c.insert(field.c_member.as_str())
        {
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
        if !claimed_c.insert(name.as_str())
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
    let all_c: BTreeSet<&str> = pair
        .c_only_members
        .iter()
        .map(|member| member.name.as_str())
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
    let skips: BTreeMap<&str, &str> = descriptor
        .skips
        .iter()
        .map(|entry| (entry.member.as_str(), entry.reason.as_str()))
        .collect();
    let handles: BTreeMap<&str, &str> = descriptor
        .handles
        .iter()
        .map(|entry| (entry.member.as_str(), entry.helper.as_str()))
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
    let default_empty: BTreeSet<&str> = descriptor
        .default_empty_sequence
        .iter()
        .map(String::as_str)
        .collect();
    let needs_arena = pair.members.iter().any(|member| {
        let idl = &member.idl[0].values[0];
        !unsupported.contains(member.member.as_str())
            && !skips.contains_key(member.member.as_str())
            && (is_idl_string(idl) || is_sequence(&idl.type_name))
    }) || !descriptor.chains.is_empty()
        || !descriptor.handle_sequences.is_empty()
        || !descriptor.handle_or_enum_unions.is_empty();
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
    let _ = writeln!(output, ") -> Result<{return_target}, E::Error> {{");

    let mut cited_required = false;
    for member in &members {
        let (idl, _) = member_values(member, dictionary)?;
        let name = &member.member;
        if skips.contains_key(name.as_str()) {
            continue;
        }
        let value_name = format!("{}_value", snake_case(name));
        if handle_or_enum.contains_key(name.as_str())
            || required_defaults.contains_key(name.as_str())
        {
            let _ = writeln!(
                output,
                "    let {value_name} = E::get_property(cx, value, \"{name}\")?;"
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
                "    let {value_name} = E::get_property(cx, value, \"{name}\")?;"
            );
        }
        if unsupported.contains(name.as_str()) {
            emit_unsupported_check(&mut output, name, &value_name);
        }
    }
    for member in &pair.idl_only_members {
        let name = &member.name;
        if skips.contains_key(name.as_str()) {
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
                "    let {value_name} = E::get_property(cx, value, \"{name}\")?;"
            );
        }
        if unsupported.contains(name.as_str()) {
            emit_unsupported_check(&mut output, name, &value_name);
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
        if let Some(helper) = handles.get(name.as_str()) {
            let _ = writeln!(output, "    let {local} = {helper}::<E>(cx, {value})?;");
        } else if let Some(helper) = handle_sequences.get(name.as_str()) {
            emit_handle_sequence_local(&mut output, name, &local, &value, helper);
        } else if let Some(policy) = handle_or_enum.get(name.as_str()) {
            emit_handle_or_enum_local(&mut output, name, &local, &value, policy);
        } else if is_idl_string(idl) {
            emit_string_local(
                &mut output,
                dictionary,
                name,
                &local,
                &value,
                string_policy[name.as_str()],
                idl,
            )?;
        } else if is_enum(report, &idl.type_name) {
            emit_enum_local(&mut output, report, dictionary, name, &local, &value, idl)?;
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
            )?;
        }
    }

    for member in &pair.idl_only_members {
        let name = &member.name;
        let value = format!("{}_value", snake_case(name));
        if let Some(policy) = flatten.get(name.as_str()) {
            emit_union_flatten_locals(&mut output, report, dictionary, &value, policy)?;
        } else if let Some(policy) = chains.get(name.as_str()) {
            emit_chain_local(&mut output, name, &value, policy);
        }
    }

    if let Some(wrapper) = &descriptor.wrapper {
        for capture in &wrapper.captures {
            if capture.source != capture.field {
                let _ = writeln!(output, "    let {} = {};", capture.field, capture.source);
            }
        }
        for capture in &wrapper.sequence_captures {
            let _ = writeln!(output, "    let {} = {}", capture.field, capture.source);
            output.push_str("        .iter()\n");
            let _ = writeln!(
                output,
                "        .filter_map(|item| (!item.{}.is_null()).then_some(item.{}))",
                capture.element_field, capture.element_field
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
        )?;
    }
    if raw_c {
        let zero: BTreeSet<&str> = descriptor.zero.iter().map(String::as_str).collect();
        for member in &pair.c_only_members {
            if zero.contains(member.name.as_str()) {
                let field = rust_field_name(&member.name, true);
                let _ = writeln!(output, "        {field}: 0,");
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

fn emit_handle_sequence_local(
    output: &mut String,
    name: &str,
    local: &str,
    value: &str,
    helper: &str,
) {
    let _ = writeln!(
        output,
        "    let {local} = if E::is_undefined(cx, {value}) {{"
    );
    output.push_str("        &[][..]\n    } else {\n");
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
    _name: &str,
    local: &str,
    value: &str,
    policy: &HandleOrEnumUnionPolicy,
) {
    output.push_str(
        "    // Policy: the handle-or-enum union preserves explicit handles and auto layout.\n",
    );
    let _ = writeln!(
        output,
        "    let {local} = if E::is_undefined(cx, {value}) || E::is_null(cx, {value}) {{"
    );
    output.push_str("        ptr::null_mut()\n");
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
            let _ = writeln!(
                output,
                "    let {field_value} = E::get_property(cx, {value}, \"{}\")?;",
                field.member
            );
            let _ = writeln!(
                output,
                "    let {local} = if E::is_undefined(cx, {field_value}) {{"
            );
            let _ = writeln!(
                output,
                "        return Err(E::type_error(cx, \"{}\"));",
                policy.unsupported_error
            );
            output.push_str("    } else {\n");
            let _ = writeln!(output, "        {helper}::<E>(cx, {field_value})?");
            output.push_str("    };\n");
            continue;
        }
        let _ = writeln!(
            output,
            "    let {field_value} = E::get_property(cx, {value}, \"{}\")?;",
            field.member
        );
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
        let _ = writeln!(
            output,
            "    let {local} = if E::is_undefined(cx, {field_value}) {{"
        );
        let _ = writeln!(output, "        {default}");
        output.push_str("    } else {\n");
        let _ = writeln!(output, "        {conversion}");
        output.push_str("    };\n");
    }
    Ok(())
}

fn emit_chain_local(output: &mut String, name: &str, value: &str, policy: &ChainPolicy) {
    let local = snake_case(name);
    let target_field = rust_field_name(&policy.field, true);
    let _ = writeln!(output, "    let {local} = E::to_str(cx, {value}, arena)?;");
    output.push_str(
        "    // B3: WGSL is represented by an arena-owned chained struct with sType set.\n",
    );
    let _ = writeln!(
        output,
        "    let {local}_source = arena.alloc_slice(vec![{} {{",
        policy.target
    );
    output.push_str("        chain: WGPUChainedStruct {\n            next: ptr::null_mut(),\n");
    let _ = writeln!(output, "            sType: {},", policy.s_type);
    output.push_str("        },\n");
    let _ = writeln!(
        output,
        "        {target_field}: WGPUStringView::from_bytes({local}.as_bytes()),"
    );
    output.push_str("    }]).as_ptr();\n");
    output
        .push_str("    // SAFETY: the arena allocation contains one initialized chained source.\n");
    let _ = writeln!(
        output,
        "    let {local}_chain = unsafe {{ ptr::addr_of!((*{local}_source).chain) }}.cast_mut();"
    );
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
    local: &str,
    value: &str,
    idl: &ValueModel,
) -> Result<(), CodegenError> {
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
    let default = if let Some(undefined) = undefined {
        enum_constant(c_type, undefined)
    } else {
        let idl_default = idl
            .default_value
            .as_deref()
            .and_then(|value| value.strip_prefix('"'))
            .and_then(|value| value.strip_suffix('"'))
            .ok_or_else(|| {
                unsupported_shape(
                    dictionary,
                    name,
                    "enum has no C undefined sentinel or IDL default",
                )
            })?;
        let value = pair
            .enum_values
            .iter()
            .find(|value| value.idl_value.as_deref() == Some(idl_default))
            .and_then(|value| value.c_value.as_deref())
            .ok_or_else(|| unsupported_shape(dictionary, name, "enum default is not joined"))?;
        enum_constant(c_type, value)
    };
    output.push_str(
        "    // B6: string enum values are joined to C values; absence uses the C sentinel.\n",
    );
    let _ = writeln!(
        output,
        "    let {local} = if E::is_undefined(cx, {value}) {{"
    );
    let _ = writeln!(output, "        {default}");
    output.push_str("    } else {\n        let enum_arena = Arena::new();\n");
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
) -> Result<(), CodegenError> {
    let nested = descriptors.get(idl.type_name.as_str()).ok_or_else(|| {
        unsupported_shape(
            dictionary,
            name,
            &format!("nested dictionary {} is not selected", idl.type_name),
        )
    })?;
    if !idl.required && c.default_value.as_deref() != Some("zero") {
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
    if idl.required {
        if nested_needs_arena {
            let _ = writeln!(
                output,
                "    let {local} = {nested_function}::<E>(cx, {value}, arena)?;"
            );
        } else {
            let _ = writeln!(
                output,
                "    let {local} = {nested_function}::<E>(cx, {value})?;"
            );
        }
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
            "        {nested_function}::<E>(cx, {value}, arena)?"
        );
    } else {
        let _ = writeln!(output, "        {nested_function}::<E>(cx, {value})?");
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
) -> Result<(), CodegenError> {
    if !is_dictionary(report, element) {
        return Err(unsupported_shape(
            dictionary,
            name,
            &format!("sequence element {element} is not a dictionary"),
        ));
    }
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
    let nested_needs_arena = descriptor_needs_arena(nested_pair, nested);
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
    if nested_needs_arena {
        let _ = writeln!(
            output,
            "            {nested_function}::<E>(cx, item, arena)"
        );
    } else {
        let _ = writeln!(output, "            {nested_function}::<E>(cx, item)");
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
    skips: &BTreeMap<&str, &str>,
    handles: &BTreeMap<&str, &str>,
    handle_sequences: &BTreeMap<&str, &str>,
    handle_or_enum: &BTreeMap<&str, &HandleOrEnumUnionPolicy>,
    required_defaults: &BTreeMap<&str, u64>,
) -> Result<(), CodegenError> {
    let (idl, c) = member_values(member, dictionary)?;
    let name = &member.member;
    let field = rust_field_name(&c.name, raw_c);
    let local = rust_field_name(name, false);
    let value = format!("{}_value", snake_case(name));
    if unsupported.contains(name.as_str()) {
        output.push_str(
            "        // SAFETY: policy permits only a joined `default: zero` C member here.\n",
        );
        let _ = writeln!(output, "        {field}: unsafe {{ std::mem::zeroed() }},");
        return Ok(());
    }
    if let Some(reason) = skips.get(name.as_str()) {
        let _ = writeln!(output, "        // Policy skip: {reason}.");
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
    if is_enum(report, &idl.type_name) || is_dictionary(report, &idl.type_name) {
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
    if idl.enforce_range {
        let conversion = match (idl.integer_width, c.integer_width) {
            (Some(64), Some(64)) => {
                let _ = writeln!(
                    output,
                    "        // R8: `[EnforceRange]` {} is checked at the 64-bit boundary.",
                    idl.type_name
                );
                format!("enforce_u64::<E>(cx, {value}, \"{name}\")?")
            }
            (Some(32), Some(64)) => {
                output.push_str(
                    "        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.\n",
                );
                format!("u64::from(enforce_u32::<E>(cx, {value}, \"{name}\")?)")
            }
            (Some(32), Some(32)) => {
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
            let default = idl.default_value.as_deref().ok_or_else(|| {
                unsupported_shape(dictionary, name, "optional integer without an IDL default")
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
    pair.members.iter().any(|member| {
        let idl = &member.idl[0].values[0];
        !unsupported.contains(member.member.as_str())
            && (is_idl_string(idl) || is_sequence(&idl.type_name))
    }) || !descriptor.chains.is_empty()
        || !descriptor.handle_sequences.is_empty()
        || !descriptor.handle_or_enum_unions.is_empty()
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
            sequence_element(&idl.type_name)
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

fn enum_constant(c_type: &str, c_value: &str) -> String {
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
}
