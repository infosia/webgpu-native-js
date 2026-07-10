use std::fs;
use std::path::{Path, PathBuf};

use webgpu_native_js_codegen::{
    generate_conversions, generate_conversions_with_policy, generate_core,
    generate_core_with_policy, generate_lifecycle_with_policy, join_inputs,
    join_inputs_with_policy, render_report, CodegenError, JoinReport,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture(name: &str) -> (String, String, String) {
    let root = fixtures();
    let read = |extension: &str| {
        fs::read_to_string(root.join(format!("{name}.{extension}"))).expect("fixture file")
    };
    (read("idl"), read("yml"), read("policy.toml"))
}

fn joined_fixture(name: &str) -> JoinReport {
    let (idl, yaml, policy) = fixture(name);
    join_inputs_with_policy(&idl, &yaml, &policy).expect("fixture joins")
}

fn pinned_inputs() -> (String, String, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root");
    let idl = fs::read_to_string(root.join("third_party/gpuweb/webgpu.idl")).expect("pinned IDL");
    let yaml = fs::read_to_string(root.join("third_party/webgpu-headers/webgpu.yml"))
        .expect("pinned YAML");
    let policy = fs::read_to_string(root.join("codegen/policy.toml")).expect("policy");
    (idl, yaml, policy)
}

fn dispatch_macro_surface(emitted: &str) -> &str {
    let start = emitted
        .find("/// Invokes a caller-supplied macro")
        .expect("dispatch enumerator documentation");
    let end = emitted[start..]
        .find("\n#[doc(hidden)]")
        .map(|offset| start + offset)
        .expect("hidden FFI callback follows enumerator");
    &emitted[start..end]
}

fn focused_sampler_lifecycle(emitted: &str) -> String {
    let payload_end = emitted
        .find("/// One release request")
        .expect("release request follows payload");
    let variant_start = emitted
        .find("    /// Release a `GPUSampler`")
        .expect("sampler variant");
    let variant_end = emitted[variant_start..]
        .find("    /// Release a command buffer.")
        .map(|offset| variant_start + offset)
        .expect("variant end");
    let arm_start = emitted
        .find("            Self::Sampler")
        .expect("sampler release arm");
    let arm_end = emitted[arm_start..]
        .find('\n')
        .map(|offset| arm_start + offset + 1)
        .expect("arm end");
    let create_start = emitted
        .find("/// Implements `GPUDevice.createSampler`.")
        .expect("create function");
    format!(
        "{}\n{}\n{}\n{}",
        emitted[..payload_end].trim_end(),
        emitted[variant_start..variant_end].trim_end(),
        emitted[arm_start..arm_end].trim_end(),
        emitted[create_start..].trim_end()
    )
}

#[test]
fn clean_join_fixture_has_no_mismatches() {
    let report = joined_fixture("clean");
    assert!(report.mismatches.is_empty(), "{:?}", report.mismatches);
    assert_eq!(report.interfaces[0].members.len(), 1);
}

#[test]
fn name_mismatch_fixture_is_loud() {
    let report = joined_fixture("name_mismatch");
    assert_eq!(report.mismatches.len(), 1);
    assert!(report.mismatches[0].message.contains("IDL-only type"));
}

#[test]
fn unknown_interface_policy_fixture_fails() {
    let (idl, yaml, policy) = fixture("unknown_interface");
    let error = join_inputs_with_policy(&idl, &yaml, &policy).expect_err("dead policy must fail");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("GPUUnknown")));
}

#[test]
fn enforce_range_fixture_marks_the_argument() {
    let report = joined_fixture("enforce_range");
    let values = &report.interfaces[0].members[0].idl[0].values;
    assert!(values
        .iter()
        .any(|value| value.name == "count" && value.enforce_range));
}

#[test]
fn clamp_fixture_is_distinct_from_enforce_range_and_matches_snapshot() {
    let report = joined_fixture("clamp");
    let member = &report.dictionaries[0].members[0].idl[0].values[0];
    assert!(member.clamp);
    assert!(!member.enforce_range);

    let (idl, yaml, policy) = fixture("clamp");
    let emitted = generate_conversions_with_policy(&idl, &yaml, &policy).expect("Clamp emission");
    let expectation = fixtures().join("clamp.rs");
    if std::env::var_os("UPDATE_DESCRIPTOR_SNAPSHOTS").is_some() {
        fs::write(&expectation, &emitted).expect("regenerate Clamp snapshot");
    }
    let expected = fs::read_to_string(expectation).expect("Clamp snapshot");
    assert_eq!(emitted, expected);

    let missing = idl.replace("[Clamp] ", "");
    let error =
        generate_conversions_with_policy(&missing, &yaml, &policy).expect_err("missing Clamp kind");
    assert!(error.to_string().contains("unsigned short"));

    let wrong_width = idl.replace("unsigned short", "unsigned long");
    let error = generate_conversions_with_policy(&wrong_width, &yaml, &policy)
        .expect_err("dead Clamp shape");
    assert!(error.to_string().contains("unsupported Clamp shape"));
}

#[test]
fn nullable_and_required_dictionary_members_remain_distinct() {
    let report = joined_fixture("nullable_required");
    let dictionary = report
        .dictionaries
        .iter()
        .find(|pair| pair.idl_name.as_deref() == Some("GPUExampleDescriptor"))
        .expect("dictionary pair");
    let buffer = &dictionary
        .members
        .iter()
        .find(|member| member.member == "buffer")
        .expect("required buffer")
        .idl[0]
        .values[0];
    let optional = &dictionary
        .members
        .iter()
        .find(|member| member.member == "optionalBuffer")
        .expect("nullable buffer")
        .idl[0]
        .values[0];
    assert!(buffer.required);
    assert!(!buffer.nullable);
    assert!(!optional.required);
    assert!(optional.nullable);
}

#[test]
fn full_pinned_inputs_parse_and_subset_join_offline() {
    let (idl, yaml, _policy) = pinned_inputs();
    let report = join_inputs(&idl, &yaml).expect("full pinned join");
    assert_eq!(report.parser.remaining_bytes, 0);
    assert_eq!(report.parser.definitions, 209);
    assert_eq!(report.interfaces.len(), 17);
    assert!(report.parser.saw_enforce_range);
    assert!(report.parser.saw_same_object);
    assert!(report.parser.saw_exposed);
}

#[test]
fn full_pinned_surface_matches_committed_artifact() {
    // This snapshot is the complete OUT_DIR artifact (dispatch plus conversions).
    // To regenerate it and the focused dispatch-macro shape snapshot, run:
    // UPDATE_FULL_SURFACE=1 cargo test -p webgpu-native-js-codegen --test fixtures full_pinned_surface_matches_committed_artifact
    let (idl, yaml, _policy) = pinned_inputs();
    let emitted = generate_core(&idl, &yaml).expect("full pinned generation");
    let expectation = fixtures().join("full_surface.rs");
    if std::env::var_os("UPDATE_FULL_SURFACE").is_some() {
        fs::write(&expectation, &emitted).expect("regenerate full-surface expectation");
        fs::write(
            fixtures().join("dispatch_surface.rs"),
            dispatch_macro_surface(&emitted),
        )
        .expect("regenerate dispatch-macro expectation");
    }
    let expected = fs::read_to_string(expectation).expect("full-surface expectation");
    assert_eq!(emitted, expected);
}

#[test]
fn generated_dispatch_macro_matches_focused_shape_fixture() {
    let (idl, yaml, _policy) = pinned_inputs();
    let emitted = generate_core(&idl, &yaml).expect("full pinned generation");
    let expected =
        fs::read_to_string(fixtures().join("dispatch_surface.rs")).expect("dispatch snapshot");
    assert_eq!(dispatch_macro_surface(&emitted), expected);
    assert_eq!(expected.matches(", unsafe fn(").count(), 105);
}

#[test]
fn focused_lifecycle_fixture_matches_end_to_end_shape() {
    // To regenerate this focused payload/create/release/label/class-spec snapshot:
    // UPDATE_LIFECYCLE=1 cargo test -p webgpu-native-js-codegen --test fixtures focused_lifecycle_fixture_matches_end_to_end_shape
    let (idl, yaml, policy) = fixture("lifecycle");
    let emitted =
        generate_lifecycle_with_policy(&idl, &yaml, &policy).expect("focused lifecycle generation");
    let focused = focused_sampler_lifecycle(&emitted);
    let expectation = fixtures().join("lifecycle.rs");
    if std::env::var_os("UPDATE_LIFECYCLE").is_some() {
        fs::write(&expectation, &focused).expect("regenerate lifecycle expectation");
    }
    let expected = fs::read_to_string(expectation).expect("lifecycle snapshot");
    assert_eq!(focused, expected);
}

#[test]
fn lifecycle_method_policy_is_checked_in_both_directions() {
    let (idl, yaml, policy) = pinned_inputs();
    let dead = policy.replace(
        "member = \"mapAsync\"\npath = \"buffer_map_async\"",
        "member = \"mapAsync\"\npath = \"buffer_map_async\"\n\n[[lifecycle.methods]]\ninterface = \"GPUBuffer\"\nmember = \"notMapAsync\"\npath = \"buffer_map_async\"",
    );
    let error = generate_core_with_policy(&idl, &yaml, &dead)
        .expect_err("dead lifecycle mapping must fail");
    assert!(error.to_string().contains("notMapAsync"), "{error}");

    let uncovered = policy.replace(
        "[[lifecycle.methods]]\ninterface = \"GPUBuffer\"\nmember = \"mapAsync\"\npath = \"buffer_map_async\"\n\n",
        "",
    );
    let error = generate_core_with_policy(&idl, &yaml, &uncovered)
        .expect_err("uncovered subset method must fail");
    assert!(error.to_string().contains("GPUBuffer.mapAsync"), "{error}");
}

#[test]
fn generated_lifecycle_covers_every_selected_class_and_retention_set() {
    let (idl, yaml, policy) = pinned_inputs();
    let emitted =
        generate_lifecycle_with_policy(&idl, &yaml, &policy).expect("full lifecycle generation");

    let classes = [
        "gpu_class",
        "adapter_class",
        "device_class",
        "buffer_class",
        "queue_class",
        "shader_module_class",
        "sampler_class",
        "texture_class",
        "texture_view_class",
        "bind_group_layout_class",
        "pipeline_layout_class",
        "bind_group_class",
        "compute_pipeline_class",
        "render_pipeline_class",
        "query_set_class",
        "command_encoder_class",
        "compute_pass_encoder_class",
        "render_pass_encoder_class",
        "command_buffer_class",
    ];
    assert_eq!(
        emitted.matches("_class<E: JsEngine + 'static>").count(),
        classes.len()
    );
    for class in classes {
        assert!(
            emitted.contains(&format!("pub(super) fn {class}<")),
            "missing generated {class}"
        );
    }
    assert!(emitted.contains(
        "MethodSpec { name: \"createRenderPipeline\", length: 1, call: device_create_render_pipeline::<E> }"
    ));
    assert!(emitted.contains(
        "MethodSpec { name: \"getBindGroupLayout\", length: 1, call: compute_pipeline_get_bind_group_layout::<E> }"
    ));
    assert!(emitted.contains(
        "MethodSpec { name: \"getBindGroupLayout\", length: 1, call: render_pipeline_get_bind_group_layout::<E> }"
    ));

    assert!(emitted.contains(
        "pub struct BindGroupPayload {\n    pub(super) bind_group: WGPUBindGroup,\n    pub(super) layout: WGPUBindGroupLayout,\n    pub(super) buffers: Vec<WGPUBuffer>,\n    pub(super) samplers: Vec<WGPUSampler>,\n    pub(super) texture_views: Vec<WGPUTextureView>,\n}"
    ));
    assert!(emitted.contains(
        "pub struct ComputePipelinePayload {\n    pub(super) pipeline: WGPUComputePipeline,\n    pub(super) module: WGPUShaderModule,\n    pub(super) layout: WGPUPipelineLayout,\n}"
    ));
    assert!(emitted.contains(
        "pub struct RenderPipelinePayload {\n    pub(super) render_pipeline: WGPURenderPipeline,\n    pub(super) vertex_module: WGPUShaderModule,\n    pub(super) fragment_module: WGPUShaderModule,\n    pub(super) layout: WGPUPipelineLayout,\n}"
    ));
    assert!(emitted.contains(
        "pub struct TextureViewPayload {\n    pub(super) texture_view: WGPUTextureView,\n    pub(super) texture: WGPUTexture,\n}"
    ));
}

#[test]
fn lifecycle_retention_is_derived_and_policy_can_only_extend_it() {
    let (idl, yaml, policy) = pinned_inputs();
    let shrunk = policy.replace(
        "[[descriptor.wrapper.captures]]\nfield = \"layout\"\nsource = \"layout\"\n\n[[descriptor.wrapper.sequence_captures]]",
        "[[descriptor.wrapper.sequence_captures]]",
    );
    assert_ne!(shrunk, policy, "bind-group capture fixture must be found");
    let error = generate_lifecycle_with_policy(&idl, &yaml, &shrunk)
        .expect_err("removing derived bind-group retention must fail");
    assert!(
        error
            .to_string()
            .contains("derived retention for GPUBindGroup differs"),
        "{error}"
    );

    let shrunk_render = policy.replace(
        "[[descriptor.wrapper.captures]]\nfield = \"fragment_module\"\nsource = \"fragment.module\"\n\n",
        "",
    );
    assert_ne!(
        shrunk_render, policy,
        "render capture fixture must be found"
    );
    let error = generate_lifecycle_with_policy(&idl, &yaml, &shrunk_render)
        .expect_err("removing derived render retention must fail");
    assert!(
        error
            .to_string()
            .contains("derived retention for GPURenderPipeline differs"),
        "{error}"
    );

    let extended = policy.replace(
        "[[lifecycle.quirks]]\ninterface = \"GPUCommandEncoder\"\nkind = \"stateful_encoder_payload\"\nreason = \"B10/B19 keep the encoder handle in shared ended-state and pass null for an omitted optional descriptor\"",
        "[[lifecycle.quirks]]\ninterface = \"GPUCommandEncoder\"\nkind = \"stateful_encoder_payload\"\nreason = \"B10/B19 keep the encoder handle in shared ended-state and pass null for an omitted optional descriptor\"\n\n[[lifecycle.retention_extensions]]\ninterface = \"GPUComputePipeline\"\nfield = \"extra_module\"\nsource = \"module\"\nhandle_type = \"WGPUShaderModule\"\nreason = \"focused proof that policy extensions are additive\"",
    );
    assert_ne!(extended, policy, "lifecycle quirk fixture must be found");
    let emitted = generate_lifecycle_with_policy(&idl, &yaml, &extended)
        .expect("reasoned retention extension");
    assert!(emitted.contains("pub(super) extra_module: WGPUShaderModule,"));
    assert!(emitted.contains("extra_module: converted.module,"));
    assert!(emitted.contains("extra_module: payload.extra_module,"));
}

#[test]
fn dispatch_policy_is_checked_in_both_directions() {
    let (idl, yaml, policy) = pinned_inputs();
    let cases = [
        (
            policy.replace(
                "  { symbol = \"wgpuAdapterRelease\", reason = \"the bootstrap adapter is outside the selected interface subset\" },\n",
                "",
            ),
            "dispatch extras must account",
        ),
        (
            policy.replace(
                "  { interface = \"GPUSampler\", reason = \"standard resource lifecycle keeps AddRef available for retained descriptors\" },\n",
                "",
            ),
            "must account for every subset interface",
        ),
        (
            policy.replace("wgpuBufferGetConstMappedRange", "wgpuBufferNoSuchRange"),
            "nonexistent symbol",
        ),
        (
            policy.replace(
                "  { interface = \"GPUBuffer\", member = \"usage\", reason = \"the immutable usage is cached from GPUBufferDescriptor\" },\n",
                "",
            ),
            "dispatch member skips must account",
        ),
    ];
    for (bad_policy, needle) in cases {
        let error = generate_core_with_policy(&idl, &yaml, &bad_policy)
            .expect_err("dispatch policy deviation must fail");
        assert!(error.to_string().contains(needle), "{error}");
    }
}

#[test]
fn new_descriptor_policy_reasons_are_surfaced_in_the_report() {
    let (idl, yaml, _policy) = pinned_inputs();
    let report = webgpu_native_js_codegen::render_report(
        &join_inputs(&idl, &yaml).expect("full pinned join"),
    );
    for reason in [
        "recorded deferral: block 03 section 7",
        "out of scope until query sets",
        "WebIDL names the reusable programmable stage",
    ] {
        assert!(report.contains(reason), "missing policy reason: {reason}");
    }
}

#[test]
fn new_descriptor_policy_kinds_reject_missing_and_dead_entries() {
    let (idl, yaml, policy) = pinned_inputs();
    let cases = [
        (
            policy.replace(
                "[[descriptor.union_flatten.fields]]\nmember = \"size\"\nc_member = \"size\"\nabsent_constant = \"WGPU_WHOLE_SIZE\"\n",
                "",
            ),
            "size",
        ),
        (
            policy.replace(
                "[[descriptor.chains]]\nmember = \"code\"\ntarget = \"WGPUShaderSourceWGSL\"\nfield = \"code\"\ns_type = \"WGPUSType_WGPUSType_ShaderSourceWGSL\"\nreason = \"B3 requires WGSL source to be represented by a typed chained struct\"\n",
                "",
            ),
            "code",
        ),
        (
            policy.replace("helper = \"bind_group_layout_handle\"", "helper = \"not a helper\""),
            "invalid",
        ),
        (
            policy.replace("enum_value = \"auto\"", "enum_value = \"missing\""),
            "missing",
        ),
        (
            policy.replace(
                "member = \"timestampWrites\"\nreason = \"out of scope until query sets\"",
                "member = \"timestampWrites\"\nreason = \"out of scope until query sets\"\n\n[[descriptor.skips]]\nmember = \"notTimestampWrites\"\nreason = \"dead test entry\"",
            ),
            "notTimestampWrites",
        ),
    ];
    for (bad_policy, needle) in cases {
        let error = generate_conversions_with_policy(&idl, &yaml, &bad_policy)
            .expect_err("policy must fail");
        assert!(error.to_string().contains(needle), "{error}");
    }
}

#[test]
fn sampler_subset_and_descriptor_policy_are_checked_in_both_directions() {
    let (idl, yaml, policy) = pinned_inputs();
    let missing_subset_member = policy.replace("  \"createSampler\",\n", "");
    let error = generate_conversions_with_policy(&idl, &yaml, &missing_subset_member)
        .expect_err("dead sampler descriptor policy");
    assert!(error.to_string().contains("GPUSamplerDescriptor"));

    let sampler_descriptor = "[[descriptor]]\ndictionary = \"GPUSamplerDescriptor\"\n\n[[descriptor.strings]]\nmember = \"label\"\nnullable = false\n\n";
    let missing_descriptor = policy.replace(sampler_descriptor, "");
    let error = generate_conversions_with_policy(&idl, &yaml, &missing_descriptor)
        .expect_err("unpoliced createSampler descriptor");
    assert!(error.to_string().contains("GPUDevice.createSampler"));
}

#[test]
fn new_descriptor_emission_shapes_match_snapshot() {
    let (idl, yaml, _policy) = pinned_inputs();
    let emitted = generate_conversions(&idl, &yaml).expect("full descriptor emission");
    let mut selected = String::new();
    for name in [
        "convert_bind_group_entry",
        "convert_pipeline_layout_descriptor",
        "convert_shader_module_descriptor",
        "convert_programmable_stage",
    ] {
        if !selected.is_empty() {
            selected.push('\n');
        }
        let marker = format!("fn {name}<");
        let function = emitted.find(&marker).expect("emitted function");
        let start = emitted[..function]
            .rfind("/// Converts")
            .expect("function documentation");
        let end = emitted[function..]
            .find("\n/// Converts")
            .map_or(emitted.len(), |offset| function + offset);
        selected.push_str(emitted[start..end].trim_end());
        selected.push('\n');
    }
    let expected = fixtures().join("descriptor_surface.rs");
    if std::env::var_os("UPDATE_DESCRIPTOR_SNAPSHOTS").is_some() {
        fs::write(&expected, &selected).expect("regenerate descriptor shape snapshot");
    }
    let expected = fs::read_to_string(expected).expect("shape snapshot");
    assert_eq!(selected, expected);
}

#[test]
fn emitted_descriptor_matches_snapshot() {
    let (idl, yaml, policy) = fixture("emission");
    let emitted = generate_conversions_with_policy(&idl, &yaml, &policy).expect("fixture emission");
    let expectation = fixtures().join("emission.rs");
    if std::env::var_os("UPDATE_DESCRIPTOR_SNAPSHOTS").is_some() {
        fs::write(&expectation, &emitted).expect("regenerate emission snapshot");
    }
    let expected = fs::read_to_string(expectation).expect("snapshot");
    assert_eq!(emitted, expected);
}

#[test]
fn emitted_dict_or_sequence_unions_match_snapshot() {
    let (idl, yaml, policy) = fixture("dict_or_sequence");
    let emitted = generate_conversions_with_policy(&idl, &yaml, &policy)
        .expect("dict-or-sequence fixture emission");
    let expectation = fixtures().join("dict_or_sequence.rs");
    if std::env::var_os("UPDATE_DICT_OR_SEQUENCE").is_some() {
        fs::write(&expectation, &emitted).expect("regenerate dict-or-sequence snapshot");
    }
    let expected = fs::read_to_string(expectation).expect("dict-or-sequence snapshot");
    assert_eq!(emitted, expected);
}

#[test]
fn dict_or_sequence_union_policy_rejects_wrong_alias_and_lengths() {
    let (idl, yaml, policy) = fixture("dict_or_sequence");
    let wrong_alias = idl.replace(
        "sequence<GPUIntegerCoordinate> or GPUExtent3DDict",
        "sequence<double> or GPUExtent3DDict",
    );
    let error = generate_conversions_with_policy(&wrong_alias, &yaml, &policy)
        .expect_err("wrong union element must fail");
    assert!(error
        .to_string()
        .contains("fields disagree with its numeric sequence element"));

    let wrong_length = policy.replace("max_length = 3", "max_length = 2");
    let error = generate_conversions_with_policy(&idl, &yaml, &wrong_length)
        .expect_err("union policy must cover every dictionary field");
    assert!(error.to_string().contains("invalid length range"));
}

#[test]
fn emitted_nested_layout_descriptors_match_snapshot() {
    let (idl, yaml, policy) = fixture("bind_group_layout");
    let emitted =
        generate_conversions_with_policy(&idl, &yaml, &policy).expect("nested fixture emission");
    let expectation = fixtures().join("bind_group_layout.rs");
    if std::env::var_os("UPDATE_DESCRIPTOR_SNAPSHOTS").is_some() {
        fs::write(&expectation, &emitted).expect("regenerate nested snapshot");
    }
    let expected = fs::read_to_string(expectation).expect("nested snapshot");
    assert_eq!(emitted, expected);
}

#[test]
fn joined_layout_model_preserves_inheritance_sentinels_and_one_sided_members() {
    let report = joined_fixture("bind_group_layout");
    let descriptor = report
        .dictionaries
        .iter()
        .find(|pair| pair.idl_name.as_deref() == Some("GPUBindGroupLayoutDescriptor"))
        .expect("layout descriptor");
    assert_eq!(descriptor.members[0].member, "label");

    let entry = report
        .dictionaries
        .iter()
        .find(|pair| pair.idl_name.as_deref() == Some("GPUBindGroupLayoutEntry"))
        .expect("layout entry");
    assert_eq!(entry.idl_only_members[0].name, "externalTexture");
    assert_eq!(entry.c_only_members[0].name, "binding_array_size");

    let enum_pair = report
        .enums
        .iter()
        .find(|pair| pair.idl_name.as_deref() == Some("GPUBufferBindingType"))
        .expect("buffer binding enum");
    assert!(enum_pair.enum_values.iter().any(|value| {
        value.idl_value.is_none() && value.c_value.as_deref() == Some("undefined")
    }));
    assert!(enum_pair.enum_values.iter().any(|value| {
        value.idl_value.is_none() && value.c_value.as_deref() == Some("binding_not_used")
    }));
}

#[test]
fn generated_enum_idl_only_values_require_a_reasoned_policy_skip() {
    let (idl, yaml, policy) = fixture("bind_group_layout");
    let idl = idl.replace(
        "    \"read-only-storage\",\n",
        "    \"read-only-storage\",\n    \"future-only\",\n",
    );

    let error = generate_conversions_with_policy(&idl, &yaml, &policy)
        .expect_err("generated IDL-only enum value must be policy-covered");
    assert!(matches!(
        error,
        CodegenError::Policy(message)
            if message.contains("unpoliced IDL-only value on generated enum GPUBufferBindingType: future-only")
    ));

    let policy = format!(
        "{policy}\n[[enum_value_skip]]\nenum = \"GPUBufferBindingType\"\nvalue = \"future-only\"\nreason = \"fixture C ABI does not expose the future value\"\n"
    );
    generate_conversions_with_policy(&idl, &yaml, &policy)
        .expect("reasoned enum-value skip permits generation");
    let report = render_report(
        &join_inputs_with_policy(&idl, &yaml, &policy).expect("policy-covered fixture join"),
    );
    assert!(report.contains(
        "enum-value GPUBufferBindingType.future-only (fixture C ABI does not expose the future value)"
    ));
}

#[test]
fn report_renders_c_only_enum_values_by_mismatch_class() {
    let report = render_report(&joined_fixture("bind_group_layout"));
    assert!(report.contains("enum GPUBufferBindingType: C-only value binding_not_used"));
}

#[test]
fn texture_format_join_covers_every_pinned_value_and_reports_only_the_sentinel() {
    let (idl, yaml, _policy) = pinned_inputs();
    let report = join_inputs(&idl, &yaml).expect("full pinned join");
    let format = report
        .enums
        .iter()
        .find(|pair| pair.idl_name.as_deref() == Some("GPUTextureFormat"))
        .expect("texture format join");
    assert_eq!(
        format
            .enum_values
            .iter()
            .filter(|value| value.idl_value.is_some())
            .count(),
        101
    );
    assert_eq!(
        format
            .enum_values
            .iter()
            .filter(|value| value.c_value.is_some())
            .count(),
        102
    );
    let rendered = render_report(&report);
    let mismatch = rendered
        .lines()
        .filter(|line| line.contains("enum GPUTextureFormat:"))
        .collect::<Vec<_>>();
    assert_eq!(
        mismatch,
        ["  enum GPUTextureFormat: C-only value undefined"]
    );
}

#[test]
fn vertex_format_join_is_exact_and_render_family_uses_generated_conversion_kinds() {
    let (idl, yaml, _policy) = pinned_inputs();
    let report = join_inputs(&idl, &yaml).expect("full pinned join");
    let format = report
        .enums
        .iter()
        .find(|pair| pair.idl_name.as_deref() == Some("GPUVertexFormat"))
        .expect("vertex format join");
    assert_eq!(
        format
            .enum_values
            .iter()
            .filter(|value| value.idl_value.is_some())
            .count(),
        41
    );
    assert_eq!(
        format
            .enum_values
            .iter()
            .filter(|value| value.c_value.is_some())
            .count(),
        41
    );
    assert!(!render_report(&report).contains("enum GPUVertexFormat:"));

    let emitted = generate_conversions(&idl, &yaml).expect("render conversions");
    for marker in [
        "fn convert_vertex_state<",
        "fn convert_primitive_state<",
        "fn convert_depth_stencil_state<",
        "fn convert_multisample_state<",
        "fn convert_fragment_state<",
        "fn convert_render_pipeline_descriptor<",
        "nullable sequence elements are C sentinel-filled struct holes",
        "an absent optional dictionary is a null pointer in the pinned C ABI",
        "an omitted optional boolean maps to WGPUOptionalBool_Undefined",
        "signed `[EnforceRange]` long is checked at the i32 boundary",
    ] {
        assert!(
            emitted.contains(marker),
            "missing generated marker {marker}"
        );
    }
}

#[test]
fn unsupported_member_policy_is_checked_in_both_directions() {
    let (idl, yaml, policy) = fixture("bind_group_layout");
    let missing = policy.replace("unsupported = [\"externalTexture\"]", "unsupported = []");
    let error =
        generate_conversions_with_policy(&idl, &yaml, &missing).expect_err("missing policy");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("externalTexture")));

    let dead = policy.replace(
        "unsupported = [\"externalTexture\"]",
        "unsupported = [\"externalTexture\", \"notAMember\"]",
    );
    let error = generate_conversions_with_policy(&idl, &yaml, &dead).expect_err("dead policy");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("notAMember")));
}

#[test]
fn c_only_zero_policy_is_checked_in_both_directions() {
    let (idl, yaml, policy) = fixture("bind_group_layout");
    let missing = policy.replace("zero = [\"binding_array_size\"]\n", "");
    let error =
        generate_conversions_with_policy(&idl, &yaml, &missing).expect_err("missing policy");
    assert!(
        matches!(error, CodegenError::Policy(message) if message.contains("binding_array_size"))
    );

    let dead = policy.replace(
        "zero = [\"binding_array_size\"]",
        "zero = [\"binding_array_size\", \"not_a_member\"]",
    );
    let error = generate_conversions_with_policy(&idl, &yaml, &dead).expect_err("dead policy");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("not_a_member")));
}

#[test]
fn missing_string_policy_is_a_loud_unpoliced_deviation() {
    let (idl, yaml, policy) = fixture("emission");
    let policy = policy.replace(
        "\n[[descriptor.strings]]\nmember = \"label\"\nnullable = false\n",
        "\n",
    );
    let error = generate_conversions_with_policy(&idl, &yaml, &policy).expect_err("missing policy");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("unpoliced string")));
}

#[test]
fn dead_string_policy_is_rejected() {
    let (idl, yaml, policy) = fixture("emission");
    let policy = policy.replace("member = \"label\"", "member = \"missing\"");
    let error = generate_conversions_with_policy(&idl, &yaml, &policy).expect_err("dead policy");
    assert!(
        matches!(error, CodegenError::Policy(message) if message.contains("dead string policy"))
    );
}

#[test]
fn emitted_rust_targets_and_vacuous_zero_entries_are_rejected() {
    let (idl, yaml, policy) = fixture("emission");
    let invalid_target = policy.replace("target = \"BufferDescriptor\"", "target = \"not valid\"");
    let error = generate_conversions_with_policy(&idl, &yaml, &invalid_target)
        .expect_err("descriptor target must be a Rust identifier");
    assert!(error
        .to_string()
        .contains("invalid descriptor target identifier"));

    let empty_zero = policy.replace(
        "target = \"BufferDescriptor\"",
        "target = \"BufferDescriptor\"\nzero = []",
    );
    let error = generate_conversions_with_policy(&idl, &yaml, &empty_zero)
        .expect_err("empty zero policy is vacuous");
    assert!(error
        .to_string()
        .contains("dead zero policy GPUBufferDescriptor"));

    let (idl, yaml, policy) = pinned_inputs();
    let invalid_chain_target = policy.replace(
        "target = \"WGPUShaderSourceWGSL\"",
        "target = \"WGPUShaderSourceWGSL::Injected\"",
    );
    let error = generate_conversions_with_policy(&idl, &yaml, &invalid_chain_target)
        .expect_err("chain target must be a Rust identifier");
    assert!(error
        .to_string()
        .contains("invalid chain target identifier"));
}
