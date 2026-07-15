//! CommonJS bootstrap assembly for host-supplied module sources.

/// Engine-neutral CommonJS registry, resolver, and require runtime.
pub const RUNTIME: &str = r#"var __factories = Object.create(null);
var __cache = Object.create(null);
var __hasOwn = Object.prototype.hasOwnProperty;

function __register(id, factory) {
    __factories[id] = factory;
}

function __candidateIds(id, from) {
    var resolved = id;
    if (id.charAt(0) === "." && from != null) {
        var slash = from.lastIndexOf("/");
        var joined = (slash < 0 ? "" : from.slice(0, slash + 1)) + id;
        var absolute = joined.charAt(0) === "/";
        var parts = joined.split("/");
        var normalized = [];
        for (var i = 0; i < parts.length; i++) {
            var part = parts[i];
            if (part === "" || part === ".") {
                continue;
            }
            if (part === "..") {
                if (normalized.length > 0 && normalized[normalized.length - 1] !== "..") {
                    normalized.pop();
                } else if (!absolute) {
                    normalized.push("..");
                }
            } else {
                normalized.push(part);
            }
        }
        resolved = (absolute ? "/" : "") + normalized.join("/");
    }
    return [resolved, resolved + ".js", resolved + "/index.js"];
}

function __resolve(id, from) {
    var candidates = __candidateIds(id, from);
    for (var i = 0; i < candidates.length; i++) {
        if (__hasOwn.call(__factories, candidates[i])) {
            return candidates[i];
        }
    }
    return null;
}

function __require(id, from) {
    var resolved = __resolve(id, from);
    if (resolved === null) {
        var candidates = __candidateIds(id, from);
        var tried = [];
        for (var i = 0; i < candidates.length; i++) {
            tried.push("'" + candidates[i] + "'");
        }
        throw new Error(
            "Cannot find module '" + id + "'" +
            (from ? " from '" + from + "'" : "") +
            " (tried " + tried.join(", ") + ")"
        );
    }
    if (__hasOwn.call(__cache, resolved)) {
        return __cache[resolved].exports;
    }
    var module = { exports: {}, id: resolved };
    __cache[resolved] = module;
    var factory = __factories[resolved];
    factory.call(
        module.exports,
        module,
        module.exports,
        function (requiredId) { return __require(requiredId, resolved); },
        resolved
    );
    return module.exports;
}
"#;

/// Escapes text for the contents of a double-quoted JavaScript string literal.
#[must_use]
pub fn escape_js_string(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let mut escaped = String::with_capacity(s.len());
    for character in s.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            '\u{0000}'..='\u{001F}' => {
                let code = character as usize;
                escaped.push_str("\\u00");
                escaped.push(char::from(HEX[(code >> 4) & 0x0F]));
                escaped.push(char::from(HEX[code & 0x0F]));
            }
            _ => escaped.push(character),
        }
    }
    escaped
}

/// Builds one self-contained script that runs host-supplied CommonJS modules.
#[must_use]
pub fn build_bootstrap(modules: &[(String, String)], entry: &str) -> String {
    let mut bootstrap = String::from("(function () {\n");
    bootstrap.push_str(RUNTIME);
    for (id, source) in modules {
        bootstrap.push_str("__register(\"");
        bootstrap.push_str(&escape_js_string(id));
        bootstrap.push_str("\", function (module, exports, require, __filename) {\n");
        bootstrap.push_str(source);
        bootstrap.push_str("\n});\n");
    }
    bootstrap.push_str("return __require(\"");
    bootstrap.push_str(&escape_js_string(entry));
    bootstrap.push_str("\");\n})()");
    bootstrap
}

#[cfg(test)]
mod tests {
    use super::{build_bootstrap, escape_js_string, RUNTIME};

    #[test]
    fn runtime_defines_registration_resolution_and_caching() {
        assert!(RUNTIME.contains("function __register(id, factory)"));
        assert!(RUNTIME.contains("function __resolve(id, from)"));
        assert!(RUNTIME.contains("function __require(id, from)"));
        assert!(RUNTIME.contains("__cache[resolved] = module"));
    }

    #[test]
    fn escape_js_string_handles_required_characters_and_preserves_text() {
        assert_eq!(
            escape_js_string("\"\\\n\r\u{2028}\u{2029}\u{0001}"),
            "\\\"\\\\\\n\\r\\u2028\\u2029\\u0001"
        );
        assert_eq!(escape_js_string("game/util-café.js"), "game/util-café.js");
    }

    #[test]
    fn build_bootstrap_embeds_ordered_modules_and_returns_entry_exports() {
        let first_source = "exports.value = require(\"second\").value + 1;";
        let second_source = "exports.value = 41;";
        let modules = vec![
            ("first".to_owned(), first_source.to_owned()),
            ("second".to_owned(), second_source.to_owned()),
        ];

        let bootstrap = build_bootstrap(&modules, "first");

        assert!(bootstrap.contains(RUNTIME));
        assert_eq!(bootstrap.matches("__register(\"").count(), modules.len());
        assert!(bootstrap.contains(first_source));
        assert!(bootstrap.contains(second_source));
        let first = bootstrap
            .find("__register(\"first\"")
            .expect("first module");
        let second = bootstrap
            .find("__register(\"second\"")
            .expect("second module");
        assert!(first < second);
        assert!(bootstrap.ends_with("return __require(\"first\");\n})()"));
    }

    #[test]
    fn build_bootstrap_escapes_module_and_entry_ids() {
        let id = "quote\"slash\\line\n";
        let modules = vec![(id.to_owned(), "module.exports = 1;".to_owned())];

        let bootstrap = build_bootstrap(&modules, id);

        assert!(bootstrap.contains("__register(\"quote\\\"slash\\\\line\\n\""));
        assert!(bootstrap.ends_with("return __require(\"quote\\\"slash\\\\line\\n\");\n})()"));
    }
}
