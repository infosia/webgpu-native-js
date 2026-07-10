//! Lifecycle and class-table emission for the selected interface surface.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::{
    snake_case, CodegenError, DescriptorEntry, IdlMemberKind, JoinReport, LifecyclePolicy,
    MemberPair, Policy, SubsetEntry, TypePair,
};

#[derive(Clone)]
struct StandardInterface<'a> {
    interface: &'a TypePair,
    create: &'a MemberPair,
    descriptor: &'a DescriptorEntry,
    handle_field: String,
    handle_type: String,
    payload: String,
    class_id: String,
    class_fn: String,
    finalizer: String,
    release_variant: String,
    release_field: String,
    release_dispatch: String,
    label: bool,
    stateful_encoder: bool,
    retained: Vec<RetainedHandle>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RetainedHandle {
    field: String,
    source: String,
    handle_type: String,
    dispatch: String,
    sequence: bool,
    nullable: bool,
}

pub(super) fn emit_lifecycle(report: &JoinReport, source: &str) -> Result<String, CodegenError> {
    let policy: Policy =
        toml::from_str(source).map_err(|error| CodegenError::Policy(error.to_string()))?;
    let Some(lifecycle) = policy.lifecycle.as_ref() else {
        return Ok(String::new());
    };
    validate_lifecycle(report, &policy, lifecycle)?;
    let standards = standard_interfaces(report, &policy, lifecycle)?;

    let mut output = String::new();
    emit_payloads(&mut output, &standards);
    emit_release_request(&mut output, &standards);
    for standard in &standards {
        emit_create(&mut output, standard)?;
        if standard.label {
            emit_label_accessors(&mut output, standard);
        }
        emit_finalizer(&mut output, standard);
    }
    emit_class_specs(&mut output, report, &policy, lifecycle, &standards)?;
    if output.ends_with("\n\n") {
        output.pop();
    }
    Ok(output)
}

fn validate_lifecycle(
    report: &JoinReport,
    policy: &Policy,
    lifecycle: &LifecyclePolicy,
) -> Result<(), CodegenError> {
    let interfaces: BTreeMap<&str, &TypePair> = report
        .interfaces
        .iter()
        .filter_map(|pair| pair.idl_name.as_deref().map(|name| (name, pair)))
        .collect();
    let subsets: BTreeMap<&str, &SubsetEntry> = policy
        .subset
        .iter()
        .map(|entry| (entry.interface.as_str(), entry))
        .collect();
    let mut standards = BTreeSet::new();
    for name in &lifecycle.standard_interfaces {
        if !standards.insert(name.as_str()) {
            return Err(CodegenError::Policy(format!(
                "duplicate standard lifecycle interface {name}"
            )));
        }
        if !subsets.contains_key(name.as_str()) {
            return Err(CodegenError::Policy(format!(
                "dead standard lifecycle interface {name}: not in the subset"
            )));
        }
        let interface = interfaces.get(name.as_str()).ok_or_else(|| {
            CodegenError::Policy(format!("standard lifecycle interface {name} is not joined"))
        })?;
        let creators: Vec<_> = report
            .interfaces
            .iter()
            .flat_map(|owner| owner.members.iter())
            .filter(|member| {
                member.idl.iter().any(|overload| {
                    overload
                        .values
                        .first()
                        .is_some_and(|value| value.type_name.trim_end_matches('?') == name.as_str())
                })
            })
            .collect();
        if creators.len() != 1 {
            return Err(CodegenError::Policy(format!(
                "standard lifecycle interface {name} needs exactly one joined creator; got {}",
                creators.len()
            )));
        }
        if interface.c_name.is_none() {
            return Err(CodegenError::Policy(format!(
                "standard lifecycle interface {name} has no C handle type"
            )));
        }
    }

    let mut methods = BTreeSet::new();
    for mapping in &lifecycle.methods {
        if mapping.path.trim().is_empty() {
            return Err(CodegenError::Policy(format!(
                "method mapping {}.{} has an empty path",
                mapping.interface, mapping.member
            )));
        }
        if !methods.insert((mapping.interface.as_str(), mapping.member.as_str())) {
            return Err(CodegenError::Policy(format!(
                "duplicate method mapping {}.{}",
                mapping.interface, mapping.member
            )));
        }
        if mapping.length.is_some() && mapping.reason.as_deref().is_none_or(str::is_empty) {
            return Err(CodegenError::Policy(format!(
                "method-length override {}.{} needs a reason",
                mapping.interface, mapping.member
            )));
        }
    }
    let mut properties = BTreeSet::new();
    for mapping in &lifecycle.properties {
        if mapping.get.trim().is_empty() {
            return Err(CodegenError::Policy(format!(
                "property mapping {}.{} has an empty getter",
                mapping.interface, mapping.member
            )));
        }
        if !properties.insert((mapping.interface.as_str(), mapping.member.as_str())) {
            return Err(CodegenError::Policy(format!(
                "duplicate property mapping {}.{}",
                mapping.interface, mapping.member
            )));
        }
    }
    let mut omitted = BTreeSet::new();
    for entry in &lifecycle.omitted_methods {
        require_reason("omitted method", &entry.reason)?;
        if !omitted.insert((entry.interface.as_str(), entry.member.as_str())) {
            return Err(CodegenError::Policy(format!(
                "duplicate omitted method {}.{}",
                entry.interface, entry.member
            )));
        }
    }
    if !methods.is_disjoint(&omitted) {
        return Err(CodegenError::Policy(
            "lifecycle method mappings and omitted methods overlap".to_owned(),
        ));
    }

    for subset in &policy.subset {
        let pair = interfaces.get(subset.interface.as_str()).ok_or_else(|| {
            CodegenError::Policy(format!(
                "lifecycle lost subset interface {}",
                subset.interface
            ))
        })?;
        for selected in &subset.members {
            let member = pair
                .members
                .iter()
                .find(|member| member.member == *selected)
                .ok_or_else(|| {
                    CodegenError::Policy(format!(
                        "lifecycle lost subset member {}.{selected}",
                        subset.interface
                    ))
                })?;
            match member.idl[0].kind {
                IdlMemberKind::Operation => {
                    let generated = member.idl.iter().any(|overload| {
                        overload.values.first().is_some_and(|value| {
                            standards.contains(value.type_name.trim_end_matches('?'))
                        })
                    });
                    let key = (subset.interface.as_str(), selected.as_str());
                    if usize::from(generated)
                        + usize::from(methods.contains(&key))
                        + usize::from(omitted.contains(&key))
                        != 1
                    {
                        return Err(CodegenError::Policy(format!(
                            "subset method {}.{selected} must have exactly one generated body, mapping, or reasoned omission",
                            subset.interface
                        )));
                    }
                }
                IdlMemberKind::Attribute => {
                    let generated_label =
                        selected == "label" && standards.contains(subset.interface.as_str());
                    let key = (subset.interface.as_str(), selected.as_str());
                    if usize::from(generated_label) + usize::from(properties.contains(&key)) != 1 {
                        return Err(CodegenError::Policy(format!(
                            "subset property {}.{selected} must have exactly one generated body or mapping",
                            subset.interface
                        )));
                    }
                }
                IdlMemberKind::DictionaryField => unreachable!("interface member"),
            }
        }
    }

    let selected_members: BTreeSet<_> = policy
        .subset
        .iter()
        .flat_map(|entry| {
            entry
                .members
                .iter()
                .map(move |member| (entry.interface.as_str(), member.as_str()))
        })
        .chain(
            lifecycle
                .extra_class_interfaces
                .iter()
                .flat_map(|interface| {
                    lifecycle
                        .methods
                        .iter()
                        .filter(move |mapping| mapping.interface == *interface)
                        .map(|mapping| (mapping.interface.as_str(), mapping.member.as_str()))
                }),
        )
        .collect();
    for key in methods
        .iter()
        .chain(properties.iter())
        .chain(omitted.iter())
    {
        if !selected_members.contains(key) {
            return Err(CodegenError::Policy(format!(
                "dead lifecycle mapping {}.{}",
                key.0, key.1
            )));
        }
    }

    let mut extensions = BTreeSet::new();
    for entry in &lifecycle.retention_extensions {
        require_reason("retention extension", &entry.reason)?;
        if !standards.contains(entry.interface.as_str()) {
            return Err(CodegenError::Policy(format!(
                "dead retention extension {}.{}: interface is not standard",
                entry.interface, entry.field
            )));
        }
        if !extensions.insert((entry.interface.as_str(), entry.field.as_str())) {
            return Err(CodegenError::Policy(format!(
                "duplicate retention extension {}.{}",
                entry.interface, entry.field
            )));
        }
    }
    for quirk in &lifecycle.quirks {
        require_reason("lifecycle quirk", &quirk.reason)?;
        if !standards.contains(quirk.interface.as_str()) {
            return Err(CodegenError::Policy(format!(
                "dead lifecycle quirk {}.{}",
                quirk.interface, quirk.kind
            )));
        }
        if quirk.kind != "null_descriptor_when_omitted" && quirk.kind != "stateful_encoder_payload"
        {
            return Err(CodegenError::Policy(format!(
                "unknown lifecycle quirk {}.{}",
                quirk.interface, quirk.kind
            )));
        }
    }
    Ok(())
}

fn standard_interfaces<'a>(
    report: &'a JoinReport,
    policy: &'a Policy,
    lifecycle: &'a LifecyclePolicy,
) -> Result<Vec<StandardInterface<'a>>, CodegenError> {
    let mut result = Vec::new();
    for name in &lifecycle.standard_interfaces {
        let interface = report
            .interfaces
            .iter()
            .find(|pair| pair.idl_name.as_deref() == Some(name))
            .expect("validated standard interface");
        let create = report
            .interfaces
            .iter()
            .flat_map(|owner| owner.members.iter())
            .find(|member| {
                member.idl.iter().any(|overload| {
                    overload
                        .values
                        .first()
                        .is_some_and(|value| value.type_name.trim_end_matches('?') == name.as_str())
                })
            })
            .expect("validated creator");
        let descriptor_name = create.idl[0]
            .values
            .get(1)
            .map(|value| value.type_name.trim_end_matches('?'))
            .ok_or_else(|| {
                CodegenError::Policy(format!(
                    "standard creator {} has no descriptor",
                    create.member
                ))
            })?;
        let descriptor = policy
            .descriptor
            .iter()
            .find(|entry| entry.dictionary == descriptor_name)
            .ok_or_else(|| {
                CodegenError::Policy(format!(
                    "standard lifecycle {} needs emitted descriptor {descriptor_name}",
                    name
                ))
            })?;
        let object = object_name(name);
        let handle_field = handle_field(object);
        let retained = derived_retention(report, policy, name, descriptor, lifecycle)?;
        result.push(StandardInterface {
            interface,
            create,
            descriptor,
            handle_type: interface.c_name.clone().expect("validated C type"),
            payload: format!("{object}Payload"),
            class_id: format!("GPU_{}_CLASS", screaming_snake(object)),
            class_fn: format!("{}_class", snake_case(object)),
            finalizer: format!("finalize_{}", snake_case(object)),
            release_variant: object.to_owned(),
            release_field: handle_field.clone(),
            release_dispatch: format!("{}_release", snake_case(object)),
            handle_field,
            label: policy
                .subset
                .iter()
                .find(|entry| entry.interface == *name)
                .is_some_and(|entry| entry.members.iter().any(|member| member == "label")),
            stateful_encoder: lifecycle
                .quirks
                .iter()
                .any(|entry| entry.interface == *name && entry.kind == "stateful_encoder_payload"),
            retained,
        });
    }
    Ok(result)
}

// The conversion wrapper is itself derived from joined handle-bearing members.
// Direct handle fields and handles reached through nested dictionaries/unions
// are captures. A sequence of nullable handles is passed to the native create
// call but is not a wrapper capture; native pipeline-layout ownership is the
// established C-ABI pattern.
fn derived_retention(
    report: &JoinReport,
    policy: &Policy,
    interface: &str,
    descriptor: &DescriptorEntry,
    lifecycle: &LifecyclePolicy,
) -> Result<Vec<RetainedHandle>, CodegenError> {
    let handles: BTreeMap<_, _> = report
        .interfaces
        .iter()
        .filter_map(|pair| pair.idl_name.as_deref().zip(pair.c_name.as_deref()))
        .collect();
    let mut found = Vec::new();
    let mut visiting = BTreeSet::new();
    let descriptors: BTreeMap<_, _> = policy
        .descriptor
        .iter()
        .map(|entry| (entry.dictionary.as_str(), entry))
        .collect();
    let model = RetentionModel {
        report,
        handles: &handles,
        descriptors: &descriptors,
    };
    derive_dictionary_handles(
        &model,
        &descriptor.dictionary,
        "",
        false,
        &mut visiting,
        &mut found,
    )?;
    let mut retained = Vec::new();
    for (handle, path, sequence, nullable) in found {
        let object = object_name(&handle);
        let source = if sequence {
            format!("{}s", snake_case(object))
        } else {
            path.rsplit('.').next().unwrap_or(&path).to_owned()
        };
        retained.push(RetainedHandle {
            field: source.clone(),
            source,
            handle_type: handles[handle.as_str()].to_owned(),
            dispatch: snake_case(object),
            sequence,
            nullable: nullable
                || descriptor.handle_or_enum_unions.iter().any(|entry| {
                    entry.handle_type == handle && path.ends_with(entry.member.as_str())
                }),
        });
    }
    let mut seen = BTreeSet::new();
    retained.retain(|entry| seen.insert(entry.field.clone()));

    let captured: BTreeSet<_> = descriptor
        .wrapper
        .iter()
        .flat_map(|wrapper| {
            wrapper
                .captures
                .iter()
                .map(|capture| capture.field.as_str())
                .chain(
                    wrapper
                        .sequence_captures
                        .iter()
                        .map(|capture| capture.field.as_str()),
                )
        })
        .collect();
    let derived: BTreeSet<_> = retained.iter().map(|entry| entry.field.as_str()).collect();
    if captured != derived {
        return Err(CodegenError::Policy(format!(
            "derived retention for {interface} differs from conversion captures: derived {derived:?}, captures {captured:?}"
        )));
    }
    if let Some(wrapper) = &descriptor.wrapper {
        let order: Vec<_> = wrapper
            .captures
            .iter()
            .map(|capture| capture.field.as_str())
            .chain(
                wrapper
                    .sequence_captures
                    .iter()
                    .map(|capture| capture.field.as_str()),
            )
            .collect();
        retained.sort_by_key(|entry| {
            order
                .iter()
                .position(|field| *field == entry.field)
                .unwrap_or(usize::MAX)
        });
    }
    for extension in lifecycle
        .retention_extensions
        .iter()
        .filter(|entry| entry.interface == interface)
    {
        retained.push(RetainedHandle {
            field: extension.field.clone(),
            source: extension.source.clone(),
            handle_type: extension.handle_type.clone(),
            dispatch: snake_case(extension.handle_type.trim_start_matches("WGPU")),
            sequence: extension.handle_type.starts_with("Vec<"),
            nullable: false,
        });
    }
    Ok(retained)
}

struct RetentionModel<'a> {
    report: &'a JoinReport,
    handles: &'a BTreeMap<&'a str, &'a str>,
    descriptors: &'a BTreeMap<&'a str, &'a DescriptorEntry>,
}

fn derive_dictionary_handles(
    model: &RetentionModel<'_>,
    dictionary: &str,
    prefix: &str,
    in_sequence: bool,
    visiting: &mut BTreeSet<String>,
    found: &mut Vec<(String, String, bool, bool)>,
) -> Result<(), CodegenError> {
    if !visiting.insert(dictionary.to_owned()) {
        return Ok(());
    }
    let pair = model
        .report
        .dictionaries
        .iter()
        .find(|pair| pair.idl_name.as_deref() == Some(dictionary))
        .ok_or_else(|| {
            CodegenError::Policy(format!("retention derivation lost dictionary {dictionary}"))
        })?;
    let skipped: BTreeSet<_> = model
        .descriptors
        .get(dictionary)
        .into_iter()
        .flat_map(|descriptor| descriptor.skips.iter().map(|skip| skip.member.as_str()))
        .collect();
    for member in &pair.members {
        if skipped.contains(member.member.as_str()) {
            continue;
        }
        let value = &member.idl[0].values[0];
        let path = if prefix.is_empty() {
            member.member.clone()
        } else {
            format!("{prefix}.{}", member.member)
        };
        if let Some(flatten) = model.descriptors.get(dictionary).and_then(|descriptor| {
            descriptor
                .union_flatten
                .iter()
                .find(|entry| entry.member == member.member)
        }) {
            derive_type_handles(model, &flatten.arm, &path, in_sequence, visiting, found)?;
            continue;
        }
        derive_type_handles(model, &value.type_name, &path, in_sequence, visiting, found)?;
    }
    for member in &pair.idl_only_members {
        if skipped.contains(member.name.as_str()) {
            continue;
        }
        let value = &member.values[0];
        let path = if prefix.is_empty() {
            member.name.clone()
        } else {
            format!("{prefix}.{}", member.name)
        };
        if let Some(flatten) = model.descriptors.get(dictionary).and_then(|descriptor| {
            descriptor
                .union_flatten
                .iter()
                .find(|entry| entry.member == member.name)
        }) {
            derive_type_handles(model, &flatten.arm, &path, in_sequence, visiting, found)?;
            continue;
        }
        derive_type_handles(model, &value.type_name, &path, in_sequence, visiting, found)?;
    }
    visiting.remove(dictionary);
    Ok(())
}

fn derive_type_handles(
    model: &RetentionModel<'_>,
    type_name: &str,
    path: &str,
    in_sequence: bool,
    visiting: &mut BTreeSet<String>,
    found: &mut Vec<(String, String, bool, bool)>,
) -> Result<(), CodegenError> {
    let identifiers = crate::type_identifiers(type_name);
    let sequence = type_name.trim_start().starts_with("sequence<");
    for identifier in identifiers {
        if model.handles.contains_key(identifier.as_str()) {
            // The established pipeline-layout create path passes a sequence of
            // nullable layout handles whose ownership is taken by native. It is
            // not wrapper-stored. Sequences reached through dictionaries/unions
            // (bind-group entries) remain derived and retained.
            if !sequence || in_sequence {
                found.push((
                    identifier,
                    path.to_owned(),
                    in_sequence || sequence,
                    type_name.contains('?'),
                ));
            }
        } else if model
            .report
            .dictionaries
            .iter()
            .any(|pair| pair.idl_name.as_deref() == Some(identifier.as_str()))
        {
            derive_dictionary_handles(
                model,
                &identifier,
                path,
                in_sequence || sequence,
                visiting,
                found,
            )?;
        } else if let Some(alias) = model.report.enums.iter().find_map(|pair| {
            pair.idl_name
                .as_deref()
                .and_then(|name| name.strip_prefix(&format!("{identifier} = ")))
        }) {
            derive_type_handles(model, alias, path, in_sequence || sequence, visiting, found)?;
        }
    }
    Ok(())
}

fn emit_payloads(output: &mut String, standards: &[StandardInterface<'_>]) {
    for standard in standards {
        let interface = standard.interface.idl_name.as_deref().unwrap_or_default();
        let _ = writeln!(output, "/// Payload stored by a `{interface}` wrapper.");
        let _ = writeln!(output, "pub struct {} {{", standard.payload);
        if standard.stateful_encoder {
            output.push_str("    pub(super) state: Arc<Mutex<CommandEncoderState>>,\n");
        } else {
            let _ = writeln!(
                output,
                "    pub(super) {}: {},",
                standard.handle_field, standard.handle_type
            );
            for retained in &standard.retained {
                let type_name = if retained.sequence {
                    format!("Vec<{}>", retained.handle_type)
                } else {
                    retained.handle_type.clone()
                };
                let _ = writeln!(output, "    pub(super) {}: {type_name},", retained.field);
            }
            if standard.label {
                output.push_str("    pub(super) label: Mutex<String>,\n");
            }
        }
        output.push_str("}\n\n");
        let _ = writeln!(
            output,
            "// SAFETY: `{}` stores WGPU handle values. Finalization only moves those values",
            standard.payload
        );
        output.push_str("// into `ReleaseRequest`; native handles are dereferenced only by\n");
        output.push_str("// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.\n");
        let _ = writeln!(output, "unsafe impl Send for {} {{}}\n", standard.payload);
    }
}

fn emit_release_request(output: &mut String, standards: &[StandardInterface<'_>]) {
    output
        .push_str("/// One release request enqueued by finalizers and drained by the host tick.\n");
    output.push_str("pub enum ReleaseRequest {\n");
    output.push_str(NONSTANDARD_RELEASE_VARIANTS_PREFIX);
    for standard in standards {
        let interface = standard.interface.idl_name.as_deref().unwrap_or_default();
        let _ = writeln!(
            output,
            "    /// Release a `{interface}` and its retained descriptor handles."
        );
        let _ = writeln!(output, "    {} {{", standard.release_variant);
        let _ = writeln!(
            output,
            "        /// Created native handle.\n        {}: {},",
            standard.release_field, standard.handle_type
        );
        for retained in &standard.retained {
            let type_name = if retained.sequence {
                format!("Vec<{}>", retained.handle_type)
            } else {
                retained.handle_type.clone()
            };
            let _ = writeln!(
                output,
                "        /// Retained descriptor handle or handles.\n        {}: {type_name},",
                retained.field
            );
        }
        output.push_str("        /// Dispatch table used on the drain thread.\n        gpu: GpuDispatch,\n    },\n");
    }
    output.push_str(NONSTANDARD_RELEASE_VARIANTS_SUFFIX);
    output.push_str("}\n\n");
    output.push_str("// SAFETY: finalizers only move WGPU handle values into this queue; native\n");
    output
        .push_str("// handles are dereferenced only by `run()` on the creating `tick()` thread.\n");
    output.push_str("unsafe impl Send for ReleaseRequest {}\n\n");
    output.push_str("impl ReleaseRequest {\n    pub(super) fn run(self) {\n        match self {\n");
    output.push_str(NONSTANDARD_RELEASE_ARMS_PREFIX);
    for standard in standards {
        let _ = write!(
            output,
            "            Self::{} {{ {},",
            standard.release_variant, standard.release_field
        );
        for retained in &standard.retained {
            let _ = write!(output, " {},", retained.field);
        }
        output.push_str(" gpu } => unsafe {\n");
        let _ = writeln!(
            output,
            "                (gpu.{})({});",
            standard.release_dispatch, standard.release_field
        );
        for retained in &standard.retained {
            if retained.sequence {
                let _ = writeln!(
                    output,
                    "                for handle in {} {{ (gpu.{}_release)(handle); }}",
                    retained.field, retained.dispatch
                );
            } else if retained.nullable {
                let _ = writeln!(
                    output,
                    "                if !{}.is_null() {{ (gpu.{}_release)({}); }}",
                    retained.field, retained.dispatch, retained.field
                );
            } else {
                let _ = writeln!(
                    output,
                    "                (gpu.{}_release)({});",
                    retained.dispatch, retained.field
                );
            }
        }
        output.push_str("            },\n");
    }
    output.push_str(NONSTANDARD_RELEASE_ARMS_SUFFIX);
    output.push_str("        }\n    }\n}\n\n");
}

fn emit_create(output: &mut String, standard: &StandardInterface<'_>) -> Result<(), CodegenError> {
    let function = format!("device_{}", snake_case(&standard.create.member));
    let descriptor_name = &standard.descriptor.dictionary;
    let convert = format!(
        "convert_{}",
        snake_case(
            descriptor_name
                .strip_prefix("GPU")
                .unwrap_or(descriptor_name)
        )
    );
    let dispatch = snake_case(standard.create.c.name.trim_start_matches("wgpu"));
    let optional = !standard.create.idl[0].values[1].required;
    let null_optional = standard.stateful_encoder;
    let converted = standard.descriptor.wrapper.is_some();
    let native_expr = if converted {
        "converted.native"
    } else {
        "native"
    };
    let _ = writeln!(
        output,
        "/// Implements `GPUDevice.{}`.",
        standard.create.member
    );
    let _ = writeln!(output, "pub fn {function}<E: JsEngine + 'static>(");
    output.push_str("    cx: E::Context<'_>,\n    this: E::Value,\n    args: &[E::Value],\n) -> Result<E::Value, E::Error> {\n");
    output.push_str("    let device = device_handle::<E>(cx, this)?;\n");
    output.push_str("    let arena = Arena::new();\n");
    if optional && null_optional {
        output.push_str("    let native = match args.first().copied() {\n");
        let _ = writeln!(output, "        Some(value) if !E::is_undefined(cx, value) => Some({convert}::<E>(cx, value, &arena)?),");
        output.push_str("        _ => None,\n    };\n");
    } else {
        if optional {
            output.push_str(
                "    let descriptor = args.first().copied().unwrap_or_else(|| E::undefined(cx));\n",
            );
        } else {
            let _ = writeln!(output, "    let descriptor = args.first().copied().ok_or_else(|| E::type_error(cx, \"{descriptor_name}\"))?;");
        }
        let variable = if converted { "converted" } else { "native" };
        let _ = writeln!(
            output,
            "    let {variable} = {convert}::<E>(cx, descriptor, &arena)?;"
        );
    }
    if standard.label && !standard.stateful_encoder {
        let _ = writeln!(
            output,
            "    let label = unsafe {{ string_view_to_owned({native_expr}.label) }};"
        );
    }
    let descriptor_pointer = if optional && null_optional {
        "native.as_ref().map_or(ptr::null(), ptr::from_ref)".to_owned()
    } else {
        format!("ptr::from_ref(&{native_expr})")
    };
    let _ = writeln!(
        output,
        "    let {} = unsafe {{ (E::environment(cx).gpu().{dispatch})(device, {descriptor_pointer}) }};",
        standard.handle_field
    );
    let _ = writeln!(output, "    if {}.is_null() {{", standard.handle_field);
    let _ = writeln!(
        output,
        "        return Err(E::operation_error(cx, \"{} returned null\"));",
        standard.create.c.name
    );
    output.push_str("    }\n");
    if !standard.retained.is_empty() {
        output.push_str("    let gpu = E::environment(cx).gpu();\n");
        output.push_str("    unsafe {\n");
        for retained in &standard.retained {
            if retained.sequence {
                let _ = writeln!(
                    output,
                    "        for handle in &converted.{} {{ (gpu.{}_add_ref)(*handle); }}",
                    retained.source, retained.dispatch
                );
            } else if retained.nullable {
                let _ = writeln!(
                    output,
                    "        if !converted.{}.is_null() {{ (gpu.{}_add_ref)(converted.{}); }}",
                    retained.source, retained.dispatch, retained.source
                );
            } else {
                let _ = writeln!(
                    output,
                    "        (gpu.{}_add_ref)(converted.{});",
                    retained.dispatch, retained.source
                );
            }
        }
        output.push_str("    }\n");
    }
    let _ = writeln!(
        output,
        "    if let Err(error) = E::register_class(cx, {}::<E>()) {{",
        standard.class_fn
    );
    emit_cleanup(output, standard, "        ", false);
    output.push_str("        return Err(error);\n    }\n");
    for retained in &standard.retained {
        if retained.sequence {
            let _ = writeln!(
                output,
                "    let retained_{} = converted.{}.clone();",
                retained.field, retained.source
            );
        } else {
            let _ = writeln!(
                output,
                "    let retained_{} = converted.{};",
                retained.field, retained.source
            );
        }
    }
    let _ = writeln!(
        output,
        "    match E::new_instance(cx, {}, Box::new({} {{",
        standard.class_id, standard.payload
    );
    if standard.stateful_encoder {
        output.push_str("        state: Arc::new(Mutex::new(CommandEncoderState {\n");
        let _ = writeln!(output, "            {},", standard.handle_field);
        output.push_str("            ended: false,\n        })),\n");
    } else {
        let _ = writeln!(output, "        {},", standard.handle_field);
        for retained in &standard.retained {
            let _ = writeln!(
                output,
                "        {}: converted.{},",
                retained.field, retained.source
            );
        }
        if standard.label {
            output.push_str("        label: Mutex::new(label),\n");
        }
    }
    output.push_str("    })) {\n        Ok(value) => Ok(value),\n        Err(error) => {\n");
    emit_cleanup(output, standard, "            ", true);
    output.push_str("            Err(error)\n        }\n    }\n}\n\n");
    Ok(())
}

fn emit_cleanup(
    output: &mut String,
    standard: &StandardInterface<'_>,
    indent: &str,
    retained_locals: bool,
) {
    let gpu = if standard.retained.is_empty() {
        "E::environment(cx).gpu()"
    } else {
        "gpu"
    };
    let _ = writeln!(output, "{indent}unsafe {{");
    let _ = writeln!(
        output,
        "{indent}    ({gpu}.{})({});",
        standard.release_dispatch, standard.handle_field
    );
    for retained in &standard.retained {
        let source = if retained_locals {
            format!("retained_{}", retained.field)
        } else {
            format!("converted.{}", retained.source)
        };
        if retained.sequence {
            let _ = writeln!(
                output,
                "{indent}    for handle in &{source} {{ ({gpu}.{}_release)(*handle); }}",
                retained.dispatch
            );
        } else if retained.nullable {
            let _ = writeln!(
                output,
                "{indent}    if !{source}.is_null() {{ ({gpu}.{}_release)({source}); }}",
                retained.dispatch
            );
        } else {
            let _ = writeln!(
                output,
                "{indent}    ({gpu}.{}_release)({source});",
                retained.dispatch
            );
        }
    }
    let _ = writeln!(output, "{indent}}}");
}

fn emit_label_accessors(output: &mut String, standard: &StandardInterface<'_>) {
    let interface = standard.interface.idl_name.as_deref().unwrap_or_default();
    let object = object_name(interface);
    let base = snake_case(object);
    let _ = writeln!(output, "/// Implements the `{interface}.label` getter.");
    let _ = writeln!(output, "pub fn {base}_label_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {{");
    let _ = writeln!(output, "    let payload = E::payload(cx, this, {}).and_then(|payload| payload.downcast_ref::<{}>()).ok_or_else(|| E::type_error(cx, \"{interface}.label called on an incompatible object\"))?;", standard.class_id, standard.payload);
    let _ = writeln!(output, "    let label = payload.label.lock().map_err(|_| E::operation_error(cx, \"{interface} label is poisoned\"))?;");
    output.push_str("    E::string(cx, &label)\n}\n\n");
    let _ = writeln!(output, "/// Implements the `{interface}.label` setter.");
    let _ = writeln!(output, "pub fn {base}_label_set<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value, value: E::Value) -> Result<(), E::Error> {{");
    output.push_str(
        "    let arena = Arena::new();\n    let new_label = E::to_str(cx, value, &arena)?;\n",
    );
    let _ = writeln!(output, "    let payload = E::payload(cx, this, {}).and_then(|payload| payload.downcast_ref::<{}>()).ok_or_else(|| E::type_error(cx, \"{interface}.label called on an incompatible object\"))?;", standard.class_id, standard.payload);
    let _ = writeln!(output, "    let mut label = payload.label.lock().map_err(|_| E::operation_error(cx, \"{interface} label is poisoned\"))?;");
    let _ = writeln!(output, "    unsafe {{ (E::environment(cx).gpu().{}_set_label)(payload.{}, WGPUStringView::from_bytes(new_label.as_bytes())); }}", snake_case(object), standard.handle_field);
    output.push_str("    new_label.clone_into(&mut label);\n    Ok(())\n}\n\n");
}

fn emit_finalizer(output: &mut String, standard: &StandardInterface<'_>) {
    let interface = standard.interface.idl_name.as_deref().unwrap_or_default();
    let _ = writeln!(
        output,
        "/// Finalizes a `{interface}` payload by enqueuing its release."
    );
    let _ = writeln!(
        output,
        "pub fn {}(payload: Box<dyn Any + Send>, env: &Environment) {{",
        standard.finalizer
    );
    let _ = writeln!(
        output,
        "    let Ok(payload) = payload.downcast::<{}>() else {{ return; }};",
        standard.payload
    );
    if standard.stateful_encoder {
        output.push_str("    let Ok(state) = payload.state.lock() else { return; };\n");
        output.push_str("    let _ = env.queue().enqueue(ReleaseRequest::CommandEncoder { encoder: state.encoder, gpu: env.gpu() });\n");
    } else {
        let _ = writeln!(
            output,
            "    let _ = env.queue().enqueue(ReleaseRequest::{} {{",
            standard.release_variant
        );
        let _ = writeln!(
            output,
            "        {}: payload.{},",
            standard.release_field, standard.handle_field
        );
        for retained in &standard.retained {
            let _ = writeln!(
                output,
                "        {}: payload.{},",
                retained.field, retained.field
            );
        }
        output.push_str("        gpu: env.gpu(),\n    });\n");
    }
    output.push_str("}\n\n");
}

fn emit_class_specs(
    output: &mut String,
    report: &JoinReport,
    policy: &Policy,
    lifecycle: &LifecyclePolicy,
    standards: &[StandardInterface<'_>],
) -> Result<(), CodegenError> {
    let standard_map: BTreeMap<_, _> = standards
        .iter()
        .map(|standard| {
            (
                standard.interface.idl_name.as_deref().unwrap_or_default(),
                standard,
            )
        })
        .collect();
    for extra in &lifecycle.extra_class_interfaces {
        emit_one_class(output, extra, None, None, lifecycle, &standard_map)?;
    }
    for subset in &policy.subset {
        let pair = report
            .interfaces
            .iter()
            .find(|pair| pair.idl_name.as_deref() == Some(&subset.interface))
            .expect("validated subset");
        emit_one_class(
            output,
            &subset.interface,
            Some(subset),
            Some(pair),
            lifecycle,
            &standard_map,
        )?;
    }
    Ok(())
}

fn emit_one_class(
    output: &mut String,
    interface: &str,
    subset: Option<&SubsetEntry>,
    pair: Option<&TypePair>,
    lifecycle: &LifecyclePolicy,
    standards: &BTreeMap<&str, &StandardInterface<'_>>,
) -> Result<(), CodegenError> {
    let object = object_name(interface);
    let class_fn = format!("{}_class", snake_case(object));
    let class_id = if interface == "GPU" {
        "GPU_CLASS".to_owned()
    } else {
        format!("GPU_{}_CLASS", screaming_snake(object))
    };
    let standard = standards.get(interface).copied();
    let _ = writeln!(
        output,
        "pub(super) fn {class_fn}<E: JsEngine + 'static>() -> &'static ClassSpec<E> {{"
    );
    let _ = writeln!(
        output,
        "    class_spec_once::<E, _>({class_id}, || ClassSpec {{"
    );
    let _ = writeln!(output, "        name: \"{interface}\",");
    let _ = writeln!(output, "        id: {class_id},");
    output.push_str("        constructor: None,\n");

    let selected = subset.map_or(&[][..], |entry| entry.members.as_slice());
    let mut properties: Vec<_> = selected
        .iter()
        .filter_map(|name| {
            pair.and_then(|pair| pair.members.iter().find(|member| member.member == **name))
                .filter(|member| member.idl[0].kind == IdlMemberKind::Attribute)
        })
        .collect();
    properties.sort_by_key(|member| {
        lifecycle
            .properties
            .iter()
            .position(|mapping| mapping.interface == interface && mapping.member == member.member)
            .unwrap_or(usize::MAX)
    });
    if properties.is_empty() {
        output.push_str("        properties: &[],\n");
    } else {
        output.push_str("        properties: Box::leak(Box::new([\n");
        for member in properties {
            let mapping = lifecycle
                .properties
                .iter()
                .find(|mapping| mapping.interface == interface && mapping.member == member.member);
            let generated_label = member.member == "label" && standard.is_some();
            let (get, set) = if generated_label {
                let base = snake_case(object);
                (
                    format!("{base}_label_get"),
                    Some(format!("{base}_label_set")),
                )
            } else {
                let mapping = mapping.expect("validated property mapping");
                (mapping.get.clone(), mapping.set.clone())
            };
            let _ = writeln!(
                output,
                "            PropertySpec {{ name: \"{}\", get: Some({get}::<E>), set: {} }},",
                member.member,
                set.map_or_else(|| "None".to_owned(), |path| format!("Some({path}::<E>)"))
            );
        }
        output.push_str("        ])),\n");
    }

    let mut methods = Vec::new();
    if let Some(pair) = pair {
        for selected in selected {
            let member = pair
                .members
                .iter()
                .find(|member| member.member == *selected)
                .expect("validated member");
            if member.idl[0].kind != IdlMemberKind::Operation
                || lifecycle
                    .omitted_methods
                    .iter()
                    .any(|entry| entry.interface == interface && entry.member == member.member)
            {
                continue;
            }
            let created = standards.values().find(|standard| {
                member.idl.iter().any(|overload| {
                    overload.values.first().is_some_and(|value| {
                        value.type_name.trim_end_matches('?')
                            == standard.interface.idl_name.as_deref().unwrap_or_default()
                    })
                })
            });
            let mapping = lifecycle
                .methods
                .iter()
                .find(|mapping| mapping.interface == interface && mapping.member == member.member);
            let path = if created.is_some() {
                format!("device_{}", snake_case(&member.member))
            } else {
                mapping.expect("validated method mapping").path.clone()
            };
            let length = mapping
                .and_then(|mapping| mapping.length)
                .unwrap_or_else(|| webidl_length(member));
            let order = mapping.map_or_else(
                || {
                    lifecycle.methods.len()
                        + created
                            .and_then(|created| {
                                lifecycle.standard_interfaces.iter().position(|name| {
                                    created.interface.idl_name.as_deref() == Some(name)
                                })
                            })
                            .unwrap_or(usize::MAX / 2)
                },
                |mapping| {
                    lifecycle
                        .methods
                        .iter()
                        .position(|candidate| std::ptr::eq(candidate, mapping))
                        .expect("mapping belongs to lifecycle policy")
                },
            );
            methods.push((order, member.member.as_str(), path, length));
        }
    } else {
        for mapping in lifecycle
            .methods
            .iter()
            .filter(|mapping| mapping.interface == interface)
        {
            let order = lifecycle
                .methods
                .iter()
                .position(|candidate| std::ptr::eq(candidate, mapping))
                .expect("mapping belongs to lifecycle policy");
            methods.push((
                order,
                mapping.member.as_str(),
                mapping.path.clone(),
                mapping.length.unwrap_or(0),
            ));
        }
    }
    methods.sort_by_key(|method| method.0);
    if methods.is_empty() {
        output.push_str("        methods: &[],\n");
    } else {
        output.push_str("        methods: Box::leak(Box::new([\n");
        for (_, name, path, length) in methods {
            let _ = writeln!(output, "            MethodSpec {{ name: \"{name}\", length: {length}, call: {path}::<E> }},");
        }
        output.push_str("        ])),\n");
    }
    let finalizer = match interface {
        "GPU" => "|_payload, _env| {}".to_owned(),
        "GPUDevice" | "GPUBuffer" => format!("finalize_{}::<E>", snake_case(object)),
        _ => format!("finalize_{}", snake_case(object)),
    };
    let _ = writeln!(output, "        finalizer: {finalizer},");
    output.push_str("    })\n}\n\n");
    Ok(())
}

fn webidl_length(member: &MemberPair) -> u8 {
    member
        .idl
        .iter()
        .map(|overload| {
            overload
                .values
                .iter()
                .skip(1)
                .take_while(|value| value.required)
                .count() as u8
        })
        .min()
        .unwrap_or(0)
}

fn require_reason(kind: &str, reason: &str) -> Result<(), CodegenError> {
    if reason.trim().is_empty() {
        Err(CodegenError::Policy(format!("{kind} has an empty reason")))
    } else {
        Ok(())
    }
}

fn handle_field(object: &str) -> String {
    match object {
        "ShaderModule" => "module",
        "BindGroupLayout" | "PipelineLayout" => "layout",
        "ComputePipeline" => "pipeline",
        "CommandEncoder" => "encoder",
        _ => return snake_case(object),
    }
    .to_owned()
}

fn object_name(interface: &str) -> &str {
    interface
        .strip_prefix("GPU")
        .filter(|value| !value.is_empty())
        .unwrap_or(interface)
}

fn screaming_snake(value: &str) -> String {
    snake_case(value).to_ascii_uppercase()
}

const NONSTANDARD_RELEASE_VARIANTS_PREFIX: &str = r#"    /// Release an adapter.
    Adapter { /// Adapter handle.
        adapter: WGPUAdapter, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release an adopted device.
    Device { /// Device handle.
        device: WGPUDevice, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a buffer and its parent device reference.
    BufferWithDeviceRef { /// Buffer handle.
        buffer: WGPUBuffer, /// Parent device handle.
        device: WGPUDevice, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a standalone buffer reference.
    Buffer { /// Buffer handle.
        buffer: WGPUBuffer, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a queue.
    Queue { /// Queue handle.
        queue: WGPUQueue, /// Dispatch table.
        gpu: GpuDispatch },
"#;

const NONSTANDARD_RELEASE_VARIANTS_SUFFIX: &str = r#"    /// Release a command buffer.
    CommandBuffer { /// Command-buffer handle.
        command_buffer: WGPUCommandBuffer, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a compute-pass encoder.
    ComputePassEncoder { /// Pass handle.
        pass: WGPUComputePassEncoder, /// Dispatch table.
        gpu: GpuDispatch },
"#;

const NONSTANDARD_RELEASE_ARMS_PREFIX: &str = r#"            Self::Adapter { adapter, gpu } => unsafe { (gpu.adapter_release)(adapter) },
            Self::Device { device, gpu } => unsafe { (gpu.device_release)(device) },
            Self::BufferWithDeviceRef { buffer, device, gpu } => unsafe { (gpu.buffer_release)(buffer); (gpu.device_release)(device); },
            Self::Buffer { buffer, gpu } => unsafe { (gpu.buffer_release)(buffer) },
            Self::Queue { queue, gpu } => unsafe { (gpu.queue_release)(queue) },
"#;

const NONSTANDARD_RELEASE_ARMS_SUFFIX: &str = r#"            Self::CommandBuffer { command_buffer, gpu } => unsafe { (gpu.command_buffer_release)(command_buffer) },
            Self::ComputePassEncoder { pass, gpu } => unsafe { (gpu.compute_pass_encoder_release)(pass) },
"#;
