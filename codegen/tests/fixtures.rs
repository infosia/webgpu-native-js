use std::fs;
use std::path::{Path, PathBuf};

use webgpu_native_js_codegen::{join_inputs, CodegenError, JoinReport};

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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root");
    let idl = fs::read_to_string(root.join("third_party/gpuweb/webgpu.idl")).expect("pinned IDL");
    let yaml = fs::read_to_string(root.join("third_party/webgpu-headers/webgpu.yml"))
        .expect("pinned YAML");
    let policy = fs::read_to_string(root.join("codegen/policy.toml")).expect("policy");
    let report = join_inputs(&idl, &yaml, &policy).expect("full pinned join");
    assert_eq!(report.parser.remaining_bytes, 0);
    assert_eq!(report.parser.definitions, 209);
    assert_eq!(report.interfaces.len(), 11);
    assert!(report.parser.saw_enforce_range);
    assert!(report.parser.saw_same_object);
    assert!(report.parser.saw_exposed);
}
