//! Dispatch-table emission from the joined subset and C-ABI YAML.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::{
    c_function_name, c_type_name, pascal_case, snake_case, CodegenError, DispatchPolicy,
    JoinReport, Policy, YamlFunction, YamlRoot, YamlValue,
};

const REQUIRED_EXTRA_SYMBOLS: [&str; 13] = [
    "wgpuAdapterGetFeatures",
    "wgpuAdapterGetInfo",
    "wgpuAdapterGetLimits",
    "wgpuAdapterInfoFreeMembers",
    "wgpuDeviceGetAdapterInfo",
    "wgpuDeviceGetFeatures",
    "wgpuDeviceGetLimits",
    "wgpuInstanceProcessEvents",
    "wgpuInstanceRequestAdapter",
    "wgpuAdapterRequestDevice",
    "wgpuAdapterRelease",
    "wgpuBufferGetConstMappedRange",
    "wgpuSupportedFeaturesFreeMembers",
];

const REQUIRED_SKIPPED_MEMBERS: [(&str, &str); 2] = [("GPUBuffer", "size"), ("GPUBuffer", "usage")];

#[derive(Clone)]
struct DispatchEntry {
    field: String,
    symbol: String,
    args: Vec<(String, RustType)>,
    result: Option<RustType>,
}

#[derive(Clone)]
struct RustType {
    base: String,
    pointer: Option<String>,
}

impl RustType {
    fn plain(base: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            pointer: None,
        }
    }

    fn render(&self, in_macro: bool) -> String {
        let base = match self.base.as_str() {
            "c_void" => "::std::ffi::c_void".to_owned(),
            value if value.starts_with("WGPU") && in_macro => format!("$crate::{value}"),
            value => value.to_owned(),
        };
        match self.pointer.as_deref() {
            Some("mutable") => format!("*mut {base}"),
            Some(_) => format!("*const {base}"),
            None => base,
        }
    }
}

pub(super) fn emit_dispatch(
    report: &JoinReport,
    yaml: &str,
    policy: &str,
) -> Result<String, CodegenError> {
    let yaml: YamlRoot =
        serde_yaml::from_str(yaml).map_err(|error| CodegenError::Yaml(error.to_string()))?;
    let policy: Policy =
        toml::from_str(policy).map_err(|error| CodegenError::Policy(error.to_string()))?;
    let dispatch = policy.dispatch.as_ref().ok_or_else(|| {
        CodegenError::Policy("missing [dispatch] policy for core artifact emission".to_owned())
    })?;
    let registry = dispatch_registry(&yaml)?;
    validate_dispatch_policy(report, dispatch, &registry)?;
    let entries = selected_entries(report, &policy, dispatch, &registry)?;
    Ok(render_dispatch(&entries))
}

fn dispatch_registry(yaml: &YamlRoot) -> Result<BTreeMap<String, DispatchEntry>, CodegenError> {
    let mut entries = BTreeMap::new();
    for function in &yaml.functions {
        insert_entry(&mut entries, function_entry(None, function))?;
    }
    for structure in yaml
        .structs
        .iter()
        .filter(|structure| structure.free_members)
    {
        let type_name = c_type_name(&structure.name);
        let symbol = format!("wgpu{}FreeMembers", pascal_case(&structure.name));
        insert_entry(
            &mut entries,
            DispatchEntry {
                field: field_name(&symbol),
                symbol,
                args: vec![(snake_case(&structure.name), RustType::plain(type_name))],
                result: None,
            },
        )?;
    }
    for object in &yaml.objects {
        for function in &object.methods {
            insert_entry(
                &mut entries,
                function_entry(Some((&object.name, function.name.as_str())), function),
            )?;
        }
        for suffix in ["AddRef", "Release"] {
            let symbol = format!("wgpu{}{suffix}", pascal_case(&object.name));
            insert_entry(
                &mut entries,
                DispatchEntry {
                    field: field_name(&symbol),
                    symbol,
                    args: vec![(
                        snake_case(&object.name),
                        RustType::plain(c_type_name(&object.name)),
                    )],
                    result: None,
                },
            )?;
        }
    }
    Ok(entries)
}

fn insert_entry(
    entries: &mut BTreeMap<String, DispatchEntry>,
    entry: DispatchEntry,
) -> Result<(), CodegenError> {
    if entries
        .insert(entry.symbol.clone(), entry.clone())
        .is_some()
    {
        return Err(CodegenError::Yaml(format!(
            "duplicate C function symbol {}",
            entry.symbol
        )));
    }
    Ok(())
}

fn function_entry(object: Option<(&str, &str)>, function: &YamlFunction) -> DispatchEntry {
    let symbol = object.map_or_else(
        || format!("wgpu{}", pascal_case(&function.name)),
        |(object, method)| c_function_name(object, method),
    );
    let mut args = Vec::new();
    if let Some((object, _)) = object {
        args.push((snake_case(object), RustType::plain(c_type_name(object))));
    }
    for value in &function.args {
        append_argument(&mut args, value);
    }
    if let Some(callback) = &function.callback {
        args.push((
            "callback_info".to_owned(),
            RustType::plain(format!(
                "WGPU{}CallbackInfo",
                pascal_case(callback.strip_prefix("callback.").unwrap_or(callback))
            )),
        ));
    }
    let result = if function.callback.is_some() {
        Some(RustType::plain("WGPUFuture"))
    } else {
        function.returns.as_ref().map(value_type)
    };
    DispatchEntry {
        field: field_name(&symbol),
        symbol,
        args,
        result,
    }
}

fn append_argument(args: &mut Vec<(String, RustType)>, value: &YamlValue) {
    let source = value.type_name.as_deref().unwrap_or("void");
    if source.starts_with("array<") {
        args.push((
            format!("{}_count", snake_case(&value.name)),
            RustType::plain("usize"),
        ));
    }
    args.push((snake_case(&value.name), value_type(value)));
}

fn value_type(value: &YamlValue) -> RustType {
    let source = value.type_name.as_deref().unwrap_or("void");
    let base = source
        .strip_prefix("array<")
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(source);
    let base = if let Some(name) = base
        .strip_prefix("object.")
        .or_else(|| base.strip_prefix("struct."))
        .or_else(|| base.strip_prefix("enum."))
        .or_else(|| base.strip_prefix("bitflag."))
    {
        c_type_name(name)
    } else {
        match base {
            "uint16" => "u16".to_owned(),
            "uint32" => "u32".to_owned(),
            "uint64" => "u64".to_owned(),
            "int16" => "i16".to_owned(),
            "int32" => "i32".to_owned(),
            "int64" => "i64".to_owned(),
            "usize" => "usize".to_owned(),
            "bool" => "WGPUBool".to_owned(),
            "float32" => "f32".to_owned(),
            "float64" => "f64".to_owned(),
            "string" | "string_with_default_empty" | "nullable_string" | "out_string" => {
                "WGPUStringView".to_owned()
            }
            "c_void" | "void" => "c_void".to_owned(),
            other => other.to_owned(),
        }
    };
    RustType {
        base,
        pointer: if source.starts_with("array<") {
            Some(
                value
                    .pointer
                    .clone()
                    .unwrap_or_else(|| "immutable".to_owned()),
            )
        } else {
            value.pointer.clone()
        },
    }
}

fn validate_dispatch_policy(
    report: &JoinReport,
    policy: &DispatchPolicy,
    registry: &BTreeMap<String, DispatchEntry>,
) -> Result<(), CodegenError> {
    let subset: BTreeSet<&str> = report
        .interfaces
        .iter()
        .filter_map(|pair| pair.idl_name.as_deref())
        .collect();
    let members: BTreeSet<(&str, &str)> = report
        .interfaces
        .iter()
        .flat_map(|interface| {
            interface.members.iter().map(move |member| {
                (
                    interface.idl_name.as_deref().unwrap_or_default(),
                    member.member.as_str(),
                )
            })
        })
        .collect();

    let mut skipped = BTreeSet::new();
    for entry in &policy.skip_members {
        require_reason("dispatch member skip", &entry.reason)?;
        let key = (entry.interface.as_str(), entry.member.as_str());
        if !members.contains(&key) {
            return Err(CodegenError::Policy(format!(
                "dead dispatch member skip {}.{}",
                entry.interface, entry.member
            )));
        }
        if !skipped.insert(key) {
            return Err(CodegenError::Policy(format!(
                "duplicate dispatch member skip {}.{}",
                entry.interface, entry.member
            )));
        }
    }
    let required_skips: BTreeSet<_> = REQUIRED_SKIPPED_MEMBERS.into_iter().collect();
    if skipped != required_skips {
        return Err(CodegenError::Policy(format!(
            "dispatch member skips must account for {required_skips:?}; got {skipped:?}"
        )));
    }

    let mut add_ref = BTreeSet::new();
    let mut no_add_ref = BTreeSet::new();
    for (entries, destination, kind) in [
        (&policy.add_ref, &mut add_ref, "AddRef"),
        (&policy.no_add_ref, &mut no_add_ref, "no-AddRef"),
    ] {
        for entry in entries {
            require_reason(&format!("dispatch {kind}"), &entry.reason)?;
            if !subset.contains(entry.interface.as_str()) {
                return Err(CodegenError::Policy(format!(
                    "dead dispatch {kind} interface {}",
                    entry.interface
                )));
            }
            if !destination.insert(entry.interface.as_str()) {
                return Err(CodegenError::Policy(format!(
                    "duplicate dispatch {kind} interface {}",
                    entry.interface
                )));
            }
        }
    }
    if !add_ref.is_disjoint(&no_add_ref) {
        return Err(CodegenError::Policy(
            "dispatch AddRef and no-AddRef interface lists overlap".to_owned(),
        ));
    }
    let accounted: BTreeSet<_> = add_ref.union(&no_add_ref).copied().collect();
    if accounted != subset {
        return Err(CodegenError::Policy(format!(
            "dispatch AddRef policy must account for every subset interface; missing {:?}",
            subset.difference(&accounted).collect::<Vec<_>>()
        )));
    }

    let mut extras = BTreeSet::new();
    for entry in &policy.extra_symbols {
        require_reason("dispatch extra symbol", &entry.reason)?;
        if !registry.contains_key(&entry.symbol) {
            return Err(CodegenError::Policy(format!(
                "dispatch extra names nonexistent symbol {}",
                entry.symbol
            )));
        }
        if !extras.insert(entry.symbol.as_str()) {
            return Err(CodegenError::Policy(format!(
                "duplicate dispatch extra symbol {}",
                entry.symbol
            )));
        }
    }
    let required_extras: BTreeSet<_> = REQUIRED_EXTRA_SYMBOLS.into_iter().collect();
    if extras != required_extras {
        return Err(CodegenError::Policy(format!(
            "dispatch extras must account for {required_extras:?}; got {extras:?}"
        )));
    }
    Ok(())
}

fn require_reason(kind: &str, reason: &str) -> Result<(), CodegenError> {
    if reason.trim().is_empty() {
        return Err(CodegenError::Policy(format!("{kind} has an empty reason")));
    }
    Ok(())
}

fn selected_entries(
    report: &JoinReport,
    policy: &Policy,
    dispatch: &DispatchPolicy,
    registry: &BTreeMap<String, DispatchEntry>,
) -> Result<Vec<DispatchEntry>, CodegenError> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for extra in &dispatch.extra_symbols {
        push_symbol(&mut entries, &mut seen, registry, &extra.symbol)?;
    }
    let skipped: BTreeSet<_> = dispatch
        .skip_members
        .iter()
        .map(|entry| (entry.interface.as_str(), entry.member.as_str()))
        .collect();
    let add_ref: BTreeSet<_> = dispatch
        .add_ref
        .iter()
        .map(|entry| entry.interface.as_str())
        .collect();
    let interfaces: BTreeMap<_, _> = report
        .interfaces
        .iter()
        .filter_map(|pair| pair.idl_name.as_deref().map(|name| (name, pair)))
        .collect();
    for subset in &policy.subset {
        let interface = interfaces.get(subset.interface.as_str()).ok_or_else(|| {
            CodegenError::Policy(format!(
                "dispatch lost subset interface {}",
                subset.interface
            ))
        })?;
        let c_name = interface.c_name.as_deref().ok_or_else(|| {
            CodegenError::Policy(format!(
                "dispatch subset interface {} has no C object",
                subset.interface
            ))
        })?;
        let object = c_name.strip_prefix("WGPU").unwrap_or(c_name);
        if add_ref.contains(subset.interface.as_str()) {
            push_symbol(
                &mut entries,
                &mut seen,
                registry,
                &format!("wgpu{object}AddRef"),
            )?;
        }
        push_symbol(
            &mut entries,
            &mut seen,
            registry,
            &format!("wgpu{object}Release"),
        )?;
        for member_name in &subset.members {
            if skipped.contains(&(subset.interface.as_str(), member_name.as_str())) {
                continue;
            }
            let member = interface
                .members
                .iter()
                .find(|member| member.member == *member_name)
                .ok_or_else(|| {
                    CodegenError::Policy(format!(
                        "needed dispatch member {}.{} is not joined",
                        subset.interface, member_name
                    ))
                })?;
            push_symbol(&mut entries, &mut seen, registry, &member.c.name)?;
        }
    }
    Ok(entries)
}

fn push_symbol(
    entries: &mut Vec<DispatchEntry>,
    seen: &mut BTreeSet<String>,
    registry: &BTreeMap<String, DispatchEntry>,
    symbol: &str,
) -> Result<(), CodegenError> {
    let entry = registry.get(symbol).ok_or_else(|| {
        CodegenError::Policy(format!("needed dispatch symbol {symbol} does not exist"))
    })?;
    if seen.insert(symbol.to_owned()) {
        entries.push(entry.clone());
    }
    Ok(())
}

fn render_dispatch(entries: &[DispatchEntry]) -> String {
    let mut output = String::new();
    output
        .push_str("/// Function-pointer dispatch for the WebGPU C ABI calls used by this slice.\n");
    output.push_str("#[derive(Clone, Copy)]\n");
    output.push_str("pub struct GpuDispatch {\n");
    for entry in entries {
        let _ = writeln!(output, "    /// `{}`.", entry.symbol);
        let _ = write!(output, "    pub {}: unsafe fn(", entry.field);
        render_types(&mut output, entry, false, false);
        output.push(')');
        if let Some(result) = &entry.result {
            let _ = write!(output, " -> {}", result.render(false));
        }
        output.push_str(",\n");
    }
    output.push_str("}\n\n");
    output.push_str("/// Invokes a caller-supplied macro with every dispatch `(field, symbol, signature)` triple.\n");
    output.push_str("#[macro_export]\n");
    output.push_str("macro_rules! for_each_gpu_dispatch_entry {\n");
    output.push_str("    ($macro:ident $(, $context:ident)?) => {\n");
    output.push_str("        $macro! {\n");
    output.push_str("            $($context;)?\n");
    render_macro_entries(&mut output, entries);
    output.push_str("        }\n    };\n}\n\n");
    output.push_str("#[doc(hidden)]\n#[macro_export]\n");
    output.push_str("macro_rules! __gpu_dispatch_from_ffi {\n");
    output.push_str("    ($ffi:ident; $(($field:ident, $symbol:ident, unsafe fn($($argument:ident: $argument_type:ty),*) $(-> $result:ty)?),)*) => {{\n");
    output
        .push_str("        $(unsafe fn $field($($argument: $argument_type),*) $(-> $result)? {\n");
    output.push_str("            unsafe { $ffi::$symbol($($argument),*) }\n");
    output.push_str("        })*\n");
    output.push_str("        $crate::GpuDispatch { $($field),* }\n");
    output.push_str("    }};\n}\n");
    output
}

fn render_macro_entries(output: &mut String, entries: &[DispatchEntry]) {
    let indent = "            ";
    for entry in entries {
        let _ = write!(
            output,
            "{indent}({}, {}, unsafe fn(",
            entry.field, entry.symbol
        );
        render_types(output, entry, true, true);
        output.push(')');
        if let Some(result) = &entry.result {
            let _ = write!(output, " -> {}", result.render(true));
        }
        output.push_str("),\n");
    }
}

fn render_types(output: &mut String, entry: &DispatchEntry, in_macro: bool, named: bool) {
    for (index, (name, type_)) in entry.args.iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        if named {
            let _ = write!(output, "{name}: ");
        }
        output.push_str(&type_.render(in_macro));
    }
}

fn field_name(symbol: &str) -> String {
    snake_case(symbol.strip_prefix("wgpu").unwrap_or(symbol))
}
