//! Deterministic Rust emission for policy-selected descriptor conversions.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::{CodegenError, DescriptorEntry, JoinReport, MemberPair, Policy, TypePair, ValueModel};

/// Emits all descriptor conversions selected by `policy` from `report`.
///
/// The descriptor name, member names, coercions, defaults, and integer widths
/// are taken from policy and the joined model. Unsupported shapes are rejected
/// instead of being approximated in generated code.
pub fn emit_conversions(report: &JoinReport, policy: &str) -> Result<String, CodegenError> {
    let policy = parse_policy(policy)?;
    validate_policy(report, &policy)?;

    let mut output = String::new();
    for (index, descriptor) in policy.descriptor.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        let pair = descriptor_pair(report, &descriptor.dictionary)?;
        output.push_str(&emit_descriptor(pair, descriptor)?);
    }
    Ok(output)
}

pub(crate) fn validate_policy(report: &JoinReport, policy: &Policy) -> Result<(), CodegenError> {
    for descriptor in &policy.descriptor {
        let pair = descriptor_pair(report, &descriptor.dictionary)?;
        validate_descriptor_policy(pair, descriptor)?;
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
    pair: &TypePair,
    descriptor: &DescriptorEntry,
) -> Result<(), CodegenError> {
    if pair.c_name.is_none() {
        return Err(CodegenError::Policy(format!(
            "descriptor {} has no joined C-ABI type",
            descriptor.dictionary
        )));
    }

    let mut strings = BTreeMap::new();
    for entry in &descriptor.strings {
        if strings.insert(entry.member.as_str(), entry).is_some() {
            return Err(CodegenError::Policy(format!(
                "duplicate string policy {}.{}",
                descriptor.dictionary, entry.member
            )));
        }
    }

    for (name, entry) in &strings {
        let Some(member) = pair.members.iter().find(|member| member.member == **name) else {
            return Err(CodegenError::Policy(format!(
                "dead string policy {}.{name}: member is not joined",
                descriptor.dictionary
            )));
        };
        let (idl, c) = member_values(member, &descriptor.dictionary)?;
        if !is_idl_string(idl) || !c.string_view {
            return Err(CodegenError::Policy(format!(
                "dead string policy {}.{name}: member is not a joined string",
                descriptor.dictionary
            )));
        }
        if entry.nullable != idl.nullable || entry.nullable != c.nullable {
            return Err(CodegenError::Policy(format!(
                "string nullability disagreement for {}.{name}: policy={}, IDL={}, C-ABI={}",
                descriptor.dictionary, entry.nullable, idl.nullable, c.nullable
            )));
        }
    }

    for member in &pair.members {
        let (idl, c) = member_values(member, &descriptor.dictionary)?;
        let is_string = is_idl_string(idl) && c.string_view;
        if is_string && !strings.contains_key(member.member.as_str()) {
            return Err(CodegenError::Policy(format!(
                "unpoliced string nullability for {}.{}",
                descriptor.dictionary, member.member
            )));
        }
    }
    Ok(())
}

fn emit_descriptor(pair: &TypePair, descriptor: &DescriptorEntry) -> Result<String, CodegenError> {
    let dictionary = &descriptor.dictionary;
    let rust_type = dictionary.strip_prefix("GPU").unwrap_or(dictionary);
    let function = format!("convert_{}", snake_case(rust_type));
    let string_policy: BTreeMap<&str, bool> = descriptor
        .strings
        .iter()
        .map(|entry| (entry.member.as_str(), entry.nullable))
        .collect();
    let mut members: Vec<&MemberPair> = pair.members.iter().collect();
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

    let mut output = String::new();
    let _ = writeln!(
        output,
        "/// Converts a JavaScript `{dictionary}` into `{rust_type}`."
    );
    let _ = writeln!(output, "pub(super) fn {function}<E: JsEngine>(");
    output.push_str("    cx: E::Context<'_>,\n");
    output.push_str("    value: E::Value,\n");
    output.push_str("    arena: &Arena,\n");
    let _ = writeln!(output, ") -> Result<{rust_type}, E::Error> {{");

    let mut cited_required = false;
    for member in &members {
        let (idl, _) = member_values(member, dictionary)?;
        let name = &member.member;
        let value_name = format!("{}_value", snake_case(name));
        if idl.required {
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
    }

    for member in &members {
        let (idl, _) = member_values(member, dictionary)?;
        if !is_idl_string(idl) {
            continue;
        }
        let name = &member.member;
        let rust_name = snake_case(name);
        let value_name = format!("{rust_name}_value");
        let nullable = string_policy[name.as_str()];
        if nullable {
            output.push_str(
                "    // B4: nullable strings default for undefined or null as classified by policy.\n",
            );
            let _ = writeln!(
                output,
                "    let {rust_name} = if E::is_undefined(cx, {value_name}) || E::is_null(cx, {value_name}) {{"
            );
            output.push_str("        None\n");
            output.push_str("    } else {\n");
            let _ = writeln!(output, "        Some(E::to_str(cx, {value_name}, arena)?)");
            output.push_str("    };\n");
        } else {
            output.push_str(
                "    // B4: non-nullable strings default only for undefined; null is stringified.\n",
            );
            let default = idl.default_value.as_deref().ok_or_else(|| {
                unsupported(
                    dictionary,
                    name,
                    "non-nullable string without an IDL default",
                )
            })?;
            if default != "\"\"" {
                return Err(unsupported(
                    dictionary,
                    name,
                    "non-empty non-nullable string default",
                ));
            }
            let _ = writeln!(
                output,
                "    let {rust_name} = if E::is_undefined(cx, {value_name}) {{"
            );
            output.push_str("        \"\"\n");
            output.push_str("    } else {\n");
            let _ = writeln!(output, "        E::to_str(cx, {value_name}, arena)?");
            output.push_str("    };\n");
        }
    }

    let _ = writeln!(output, "    Ok({rust_type} {{");
    for member in &members {
        emit_field(&mut output, dictionary, member, &string_policy)?;
    }
    output.push_str("    })\n");
    output.push_str("}\n");
    Ok(output)
}

fn emit_field(
    output: &mut String,
    dictionary: &str,
    member: &MemberPair,
    string_policy: &BTreeMap<&str, bool>,
) -> Result<(), CodegenError> {
    let (idl, c) = member_values(member, dictionary)?;
    let name = &member.member;
    let field = snake_case(name);
    let value = format!("{field}_value");
    if is_idl_string(idl) && c.string_view {
        if string_policy[name.as_str()] {
            let _ = writeln!(output, "        {field}: {field}.map(str::to_owned),");
        } else {
            let _ = writeln!(output, "        {field}: {field}.to_owned(),");
        }
        return Ok(());
    }
    if idl.type_name == "boolean" && c.type_name == "WGPUBool" {
        output.push_str(
            "        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.\n",
        );
        if idl.required {
            let _ = writeln!(output, "        {field}: E::to_bool(cx, {value}),");
        } else {
            let default = idl.default_value.as_deref().ok_or_else(|| {
                unsupported(dictionary, name, "optional boolean without an IDL default")
            })?;
            if default != "false" {
                return Err(unsupported(
                    dictionary,
                    name,
                    "boolean default other than false",
                ));
            }
            let _ = writeln!(
                output,
                "        {field}: if E::is_undefined(cx, {value}) {{"
            );
            output.push_str("            false\n");
            output.push_str("        } else {\n");
            let _ = writeln!(output, "            E::to_bool(cx, {value})");
            output.push_str("        },\n");
        }
        return Ok(());
    }
    if idl.enforce_range {
        match (idl.integer_width, c.integer_width) {
            (Some(64), Some(64)) => {
                let _ = writeln!(
                    output,
                    "        // R8: `[EnforceRange]` {} is checked at the 64-bit boundary.",
                    idl.type_name
                );
                let _ = writeln!(
                    output,
                    "        {field}: enforce_u64::<E>(cx, {value}, \"{name}\")?,"
                );
                return Ok(());
            }
            (Some(32), Some(64)) => {
                output.push_str(
                    "        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.\n",
                );
                let _ = writeln!(
                    output,
                    "        {field}: u64::from(enforce_u32::<E>(cx, {value}, \"{name}\")?),"
                );
                return Ok(());
            }
            (Some(32), Some(32)) => {
                let _ = writeln!(
                    output,
                    "        // R8: `[EnforceRange]` {} is checked at the 32-bit boundary.",
                    idl.type_name
                );
                let _ = writeln!(
                    output,
                    "        {field}: enforce_u32::<E>(cx, {value}, \"{name}\")?,"
                );
                return Ok(());
            }
            _ => {}
        }
    }
    Err(unsupported(
        dictionary,
        name,
        &format!(
            "shape IDL={} C-ABI={} default={:?}",
            idl.type_name, c.type_name, idl.default_value
        ),
    ))
}

fn member_values<'a>(
    member: &'a MemberPair,
    dictionary: &str,
) -> Result<(&'a ValueModel, &'a ValueModel), CodegenError> {
    let idl = member
        .idl
        .first()
        .and_then(|model| model.values.first())
        .ok_or_else(|| unsupported(dictionary, &member.member, "missing IDL value"))?;
    if member.idl.len() != 1 || member.idl[0].values.len() != 1 || member.c.values.len() != 1 {
        return Err(unsupported(
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

fn unsupported(dictionary: &str, member: &str, shape: &str) -> CodegenError {
    CodegenError::Policy(format!(
        "unsupported emitted member {dictionary}.{member}: {shape}"
    ))
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
