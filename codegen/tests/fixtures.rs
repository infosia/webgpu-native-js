use std::fs;
use std::path::{Path, PathBuf};

use webgpu_native_js_codegen::{generate_conversions, join_inputs, CodegenError, JoinReport};

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
    join_inputs(&idl, &yaml, &policy).expect("fixture joins")
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
    let error = join_inputs(&idl, &yaml, &policy).expect_err("dead policy must fail");
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
    let emitted = generate_conversions(&idl, &yaml, &policy).expect("Clamp emission");
    let expected = fs::read_to_string(fixtures().join("clamp.rs")).expect("Clamp snapshot");
    assert_eq!(emitted, expected);

    let missing = idl.replace("[Clamp] ", "");
    let error = generate_conversions(&missing, &yaml, &policy).expect_err("missing Clamp kind");
    assert!(error.to_string().contains("unsigned short"));

    let wrong_width = idl.replace("unsigned short", "unsigned long");
    let error = generate_conversions(&wrong_width, &yaml, &policy).expect_err("dead Clamp shape");
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
    let (idl, yaml, policy) = pinned_inputs();
    let report = join_inputs(&idl, &yaml, &policy).expect("full pinned join");
    assert_eq!(report.parser.remaining_bytes, 0);
    assert_eq!(report.parser.definitions, 209);
    assert_eq!(report.interfaces.len(), 12);
    assert!(report.parser.saw_enforce_range);
    assert!(report.parser.saw_same_object);
    assert!(report.parser.saw_exposed);
}

#[test]
fn new_descriptor_policy_reasons_are_surfaced_in_the_report() {
    let (idl, yaml, policy) = pinned_inputs();
    let report = webgpu_native_js_codegen::render_report(
        &join_inputs(&idl, &yaml, &policy).expect("full pinned join"),
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
        let error = generate_conversions(&idl, &yaml, &bad_policy).expect_err("policy must fail");
        assert!(error.to_string().contains(needle), "{error}");
    }
}

#[test]
fn sampler_subset_and_descriptor_policy_are_checked_in_both_directions() {
    let (idl, yaml, policy) = pinned_inputs();
    let missing_subset_member = policy.replace("  \"createSampler\",\n", "");
    let error = generate_conversions(&idl, &yaml, &missing_subset_member)
        .expect_err("dead sampler descriptor policy");
    assert!(error.to_string().contains("GPUSamplerDescriptor"));

    let sampler_descriptor = "[[descriptor]]\ndictionary = \"GPUSamplerDescriptor\"\n\n[[descriptor.strings]]\nmember = \"label\"\nnullable = false\n\n";
    let missing_descriptor = policy.replace(sampler_descriptor, "");
    let error = generate_conversions(&idl, &yaml, &missing_descriptor)
        .expect_err("unpoliced createSampler descriptor");
    assert!(error.to_string().contains("GPUDevice.createSampler"));
}

#[test]
fn new_descriptor_emission_shapes_match_snapshot() {
    let (idl, yaml, policy) = pinned_inputs();
    let emitted = generate_conversions(&idl, &yaml, &policy).expect("full descriptor emission");
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
    let expected =
        fs::read_to_string(fixtures().join("descriptor_surface.rs")).expect("shape snapshot");
    assert_eq!(selected, expected);
}

#[test]
fn emitted_descriptor_matches_snapshot() {
    let (idl, yaml, policy) = fixture("emission");
    let emitted = generate_conversions(&idl, &yaml, &policy).expect("fixture emission");
    let expected = fs::read_to_string(fixtures().join("emission.rs")).expect("snapshot");
    assert_eq!(emitted, expected);
}

#[test]
fn emitted_nested_layout_descriptors_match_snapshot() {
    let (idl, yaml, policy) = fixture("bind_group_layout");
    let emitted = generate_conversions(&idl, &yaml, &policy).expect("nested fixture emission");
    let expected =
        fs::read_to_string(fixtures().join("bind_group_layout.rs")).expect("nested snapshot");
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
fn unsupported_member_policy_is_checked_in_both_directions() {
    let (idl, yaml, policy) = fixture("bind_group_layout");
    let missing = policy.replace(
        "[\"sampler\", \"texture\", \"storageTexture\", \"externalTexture\"]",
        "[\"sampler\", \"texture\", \"storageTexture\"]",
    );
    let error = generate_conversions(&idl, &yaml, &missing).expect_err("missing policy");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("externalTexture")));

    let dead = policy.replace(
        "\"externalTexture\"]",
        "\"externalTexture\", \"notAMember\"]",
    );
    let error = generate_conversions(&idl, &yaml, &dead).expect_err("dead policy");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("notAMember")));
}

#[test]
fn c_only_zero_policy_is_checked_in_both_directions() {
    let (idl, yaml, policy) = fixture("bind_group_layout");
    let missing = policy.replace("zero = [\"binding_array_size\"]\n", "");
    let error = generate_conversions(&idl, &yaml, &missing).expect_err("missing policy");
    assert!(
        matches!(error, CodegenError::Policy(message) if message.contains("binding_array_size"))
    );

    let dead = policy.replace(
        "zero = [\"binding_array_size\"]",
        "zero = [\"binding_array_size\", \"not_a_member\"]",
    );
    let error = generate_conversions(&idl, &yaml, &dead).expect_err("dead policy");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("not_a_member")));
}

#[test]
fn missing_string_policy_is_a_loud_unpoliced_deviation() {
    let (idl, yaml, policy) = fixture("emission");
    let policy = policy.replace(
        "\n[[descriptor.strings]]\nmember = \"label\"\nnullable = false\n",
        "\n",
    );
    let error = generate_conversions(&idl, &yaml, &policy).expect_err("missing policy");
    assert!(matches!(error, CodegenError::Policy(message) if message.contains("unpoliced string")));
}

#[test]
fn dead_string_policy_is_rejected() {
    let (idl, yaml, policy) = fixture("emission");
    let policy = policy.replace("member = \"label\"", "member = \"missing\"");
    let error = generate_conversions(&idl, &yaml, &policy).expect_err("dead policy");
    assert!(
        matches!(error, CodegenError::Policy(message) if message.contains("dead string policy"))
    );
}
