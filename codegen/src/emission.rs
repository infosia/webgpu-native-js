//! Deterministic Rust emission for policy-selected descriptor conversions.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::{CodegenError, DescriptorEntry, JoinReport, MemberPair, Policy, TypePair, ValueModel};

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
        if entry.nullable != idl.nullable || entry.nullable != c.nullable {
            return Err(CodegenError::Policy(format!(
                "string nullability disagreement for {}.{}: policy={}, IDL={}, C-ABI={}",
                descriptor.dictionary, entry.member, entry.nullable, idl.nullable, c.nullable
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
        if !unsupported.contains(member.name.as_str()) {
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

    for member in &pair.c_only_members {
        if !zero.contains(member.name.as_str()) {
            return Err(CodegenError::Policy(format!(
                "unpoliced C-only member {}.{}",
                descriptor.dictionary, member.name
            )));
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
    let default_empty: BTreeSet<&str> = descriptor
        .default_empty_sequence
        .iter()
        .map(String::as_str)
        .collect();
    let needs_arena = pair.members.iter().any(|member| {
        let idl = &member.idl[0].values[0];
        !unsupported.contains(member.member.as_str())
            && (is_idl_string(idl) || is_sequence(&idl.type_name))
    });

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
        "/// Converts a JavaScript `{dictionary}` into `{target}`."
    );
    let _ = writeln!(output, "pub(super) fn {function}<E: JsEngine>(");
    output.push_str("    cx: E::Context<'_>,\n");
    output.push_str("    value: E::Value,\n");
    if needs_arena {
        output.push_str("    arena: &Arena,\n");
    }
    let _ = writeln!(output, ") -> Result<{target}, E::Error> {{");

    let mut cited_required = false;
    for member in &members {
        let (idl, _) = member_values(member, dictionary)?;
        let name = &member.member;
        let value_name = format!("{}_value", snake_case(name));
        if idl.required && !default_empty.contains(name.as_str()) {
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
        let value_name = format!("{}_value", snake_case(name));
        let _ = writeln!(
            output,
            "    let {value_name} = E::get_property(cx, value, \"{name}\")?;"
        );
        emit_unsupported_check(&mut output, name, &value_name);
    }

    for member in &members {
        if unsupported.contains(member.member.as_str()) {
            continue;
        }
        let (idl, c) = member_values(member, dictionary)?;
        let name = &member.member;
        let local = rust_field_name(name, false);
        let value = format!("{}_value", snake_case(name));
        if is_idl_string(idl) {
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

    let _ = writeln!(output, "    Ok({target} {{");
    if raw_c && pair.c_chained {
        output.push_str("        nextInChain: ptr::null_mut(),\n");
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
    }
    output.push_str("    })\n");
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
    if c.default_value.as_deref() != Some("zero") {
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

fn emit_field(
    output: &mut String,
    report: &JoinReport,
    dictionary: &str,
    member: &MemberPair,
    raw_c: bool,
    string_policy: &BTreeMap<&str, bool>,
    unsupported: &BTreeSet<&str>,
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
    if is_idl_string(idl) && c.string_view {
        if raw_c {
            if string_policy[name.as_str()] {
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
        } else if string_policy[name.as_str()] {
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
        if idl.required {
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
