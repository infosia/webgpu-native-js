//! Prints the typed join report for the repository's pinned inputs.

use std::fs;
use std::process::ExitCode;

use webgpu_native_js_codegen::{join_inputs, render_report};

fn main() -> ExitCode {
    match run() {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("codegen report failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("codegen crate has no repository parent")?;
    let idl = fs::read_to_string(root.join("third_party/gpuweb/webgpu.idl"))?;
    let yaml = fs::read_to_string(root.join("third_party/webgpu-headers/webgpu.yml"))?;
    let policy = fs::read_to_string(root.join("codegen/policy.toml"))?;
    Ok(render_report(&join_inputs(&idl, &yaml, &policy)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_reads_the_pinned_repository_inputs() {
        let report = run().expect("pinned report");
        assert!(report.contains("definitions: 209 (remaining bytes: 0)"));
    }
}
