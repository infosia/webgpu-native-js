use std::path::{Path, PathBuf};

pub(crate) fn clang_target(cargo_target: &str) -> String {
    cargo_target.strip_suffix("-apple-ios-sim").map_or_else(
        || cargo_target.to_owned(),
        |arch| format!("{arch}-apple-ios-simulator"),
    )
}

pub(crate) fn derive_android_host_tag(
    host: &str,
    available_tags: &[String],
) -> Result<String, String> {
    let platform = if host.contains("apple-darwin") {
        "darwin"
    } else if host.contains("linux") {
        "linux"
    } else if host.contains("windows") {
        "windows"
    } else {
        return Err(format!("unsupported build host '{host}'"));
    };

    let prefix = format!("{platform}-");
    let mut platform_tags: Vec<&str> = available_tags
        .iter()
        .map(String::as_str)
        .filter(|tag| tag.starts_with(&prefix))
        .collect();
    platform_tags.sort_unstable();

    if platform_tags.len() == 1 {
        return Ok(platform_tags[0].to_owned());
    }
    if platform_tags.is_empty() {
        return Err(format!(
            "the NDK has no prebuilt toolchain for host platform '{platform}'"
        ));
    }

    let host_arch = host.split('-').next().unwrap_or_default();
    let preferred_arches: &[&str] = match host_arch {
        "aarch64" => &["aarch64", "arm64"],
        "x86_64" => &["x86_64"],
        other => &[other],
    };
    let architecture_matches: Vec<&str> = platform_tags
        .iter()
        .copied()
        .filter(|tag| {
            preferred_arches
                .iter()
                .any(|arch| *tag == format!("{platform}-{arch}"))
        })
        .collect();

    if architecture_matches.len() == 1 {
        return Ok(architecture_matches[0].to_owned());
    }

    Err(format!(
        "the NDK has multiple prebuilt toolchains for host platform '{platform}' and none can be selected unambiguously: {}",
        platform_tags.join(", ")
    ))
}

pub(crate) fn android_sysroot_path(ndk_root: &Path, host_tag: &str) -> PathBuf {
    ndk_root
        .join("toolchains")
        .join("llvm")
        .join("prebuilt")
        .join(host_tag)
        .join("sysroot")
}

#[cfg(test)]
mod tests {
    use super::{android_sysroot_path, clang_target, derive_android_host_tag};
    use std::path::{Path, PathBuf};

    fn tags(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn preserves_cargo_target_when_clang_uses_the_same_spelling() {
        assert_eq!(
            clang_target("aarch64-linux-android"),
            "aarch64-linux-android"
        );
    }

    #[test]
    fn translates_rust_ios_simulator_suffix_for_clang() {
        assert_eq!(
            clang_target("aarch64-apple-ios-sim"),
            "aarch64-apple-ios-simulator"
        );
    }

    #[test]
    fn derives_linux_host_tag_from_available_directories() {
        let available = tags(&["darwin-x86_64", "linux-x86_64", "windows-x86_64"]);

        assert_eq!(
            derive_android_host_tag("x86_64-unknown-linux-gnu", &available),
            Ok("linux-x86_64".to_owned())
        );
    }

    #[test]
    fn accepts_x86_64_darwin_ndk_on_apple_silicon() {
        let available = tags(&["darwin-x86_64"]);

        assert_eq!(
            derive_android_host_tag("aarch64-apple-darwin", &available),
            Ok("darwin-x86_64".to_owned())
        );
    }

    #[test]
    fn prefers_matching_architecture_when_multiple_tags_exist() {
        let available = tags(&["darwin-x86_64", "darwin-arm64"]);

        assert_eq!(
            derive_android_host_tag("aarch64-apple-darwin", &available),
            Ok("darwin-arm64".to_owned())
        );
    }

    #[test]
    fn rejects_missing_host_platform() {
        let available = tags(&["darwin-x86_64"]);

        assert_eq!(
            derive_android_host_tag("x86_64-unknown-linux-gnu", &available),
            Err("the NDK has no prebuilt toolchain for host platform 'linux'".to_owned())
        );
    }

    #[test]
    fn constructs_sysroot_path_from_ndk_root_and_host_tag() {
        assert_eq!(
            android_sysroot_path(Path::new("ndk-root"), "darwin-x86_64"),
            PathBuf::from("ndk-root/toolchains/llvm/prebuilt/darwin-x86_64/sysroot")
        );
    }
}
