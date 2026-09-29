//! Pure helpers over a crate's Cargo.toml text and user-supplied paths. Kept
//! free of IO so the rules are unit-testable without a host.

/// `MAJOR.MINOR.PATCH[-PRERELEASE]`. Build metadata (`+…`) is rejected because
/// `+` is not a valid character in an OCI tag.
pub fn is_semver(v: &str) -> bool {
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let core_ok = parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && !(p.len() > 1 && p.starts_with('0'))
        });
    let pre_ok = pre.is_none_or(|p| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    });
    core_ok && pre_ok
}

/// Accept only paths that stay inside the working directory.
pub fn check_relative(path: &str) -> Result<(), String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("Path must not be empty.".to_string());
    }
    if p.starts_with('/') || p.starts_with('\\') || p.contains(':') {
        return Err(format!(
            "Path '{path}' must be relative to the working directory."
        ));
    }
    if p.split(['/', '\\']).any(|c| c == "..") {
        return Err(format!(
            "Path '{path}' must not leave the working directory."
        ));
    }
    Ok(())
}

/// Replace `version = "…"` in the `[package]` table, preserving every other
/// byte (indentation and line endings included).
pub fn set_package_version(toml: &str, version: &str) -> Result<String, String> {
    let mut out = String::with_capacity(toml.len() + 8);
    let mut in_package = false;
    let mut replaced = false;
    for line in toml.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
        } else if in_package && !replaced {
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim_start();
                if rest.starts_with('.') {
                    return Err("Cargo.toml inherits its version from the workspace; \
                                set an explicit `version` in [package]."
                        .to_string());
                }
                if let Some(after_eq) = rest.strip_prefix('=') {
                    let value = after_eq.trim();
                    if !value.starts_with('"') {
                        if value.starts_with('{') {
                            return Err("Cargo.toml inherits its version from the workspace; \
                                        set an explicit `version` in [package]."
                                .to_string());
                        } else {
                            return Err(
                                "The `version` in [package] must be a string literal.".to_string()
                            );
                        }
                    }
                    let indent = &line[..line.len() - line.trim_start().len()];
                    let eol = if line.ends_with("\r\n") {
                        "\r\n"
                    } else if line.ends_with('\n') {
                        "\n"
                    } else {
                        ""
                    };
                    out.push_str(&format!("{indent}version = \"{version}\"{eol}"));
                    replaced = true;
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    if !replaced {
        return Err("No `version` key found in the [package] table of Cargo.toml.".to_string());
    }
    Ok(out)
}

/// The built component's file stem: `[lib].name`, else `[package].name`, with
/// dashes turned into underscores the way cargo names artifacts.
pub fn artifact_name(toml: &str) -> Option<String> {
    table_string(toml, "[lib]", "name")
        .or_else(|| table_string(toml, "[package]", "name"))
        .map(|n| n.replace('-', "_"))
}

fn table_string(toml: &str, table: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == table;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(rest) = t.strip_prefix(key) {
            if let Some(v) = rest.trim_start().strip_prefix('=') {
                if let Some(s) = v.trim().strip_prefix('"') {
                    return s.split('"').next().map(str::to_string);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRATE: &str = "[package]\nname = \"moonlit-plugin-git\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\nname = \"git\"\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nregex = { version = \"1\" }\n";

    #[test]
    fn semver_accepts_release_and_prerelease() {
        assert!(is_semver("0.1.0"));
        assert!(is_semver("10.20.30"));
        assert!(is_semver("1.0.0-beta.1"));
    }

    #[test]
    fn semver_rejects_malformed() {
        for v in [
            "",
            "1",
            "1.0",
            "1.0.0.0",
            "01.0.0",
            "1.0.0-",
            "1.0.0+build",
            "v1.0.0",
            "1.0.0-be ta",
        ] {
            assert!(!is_semver(v), "{v:?} should be rejected");
        }
    }

    #[test]
    fn escaping_paths_are_rejected() {
        assert!(check_relative("git").is_ok());
        assert!(check_relative("semantic-release/sub").is_ok());
        assert!(check_relative(".").is_ok());
        for p in [
            "",
            "  ",
            "../git",
            "git/../../x",
            "/etc/passwd",
            "\\x",
            "C:\\x",
            "C:/x",
        ] {
            assert!(check_relative(p).is_err(), "{p:?} should be rejected");
        }
    }

    #[test]
    fn sets_only_the_package_version() {
        let out = set_package_version(CRATE, "0.2.0").unwrap();
        assert!(out.contains("\nversion = \"0.2.0\"\n"));
        assert!(
            out.contains("regex = { version = \"1\" }"),
            "dependency versions untouched"
        );
        assert_eq!(out.replace("0.2.0", "0.1.0"), CRATE, "nothing else changed");
    }

    #[test]
    fn keeps_crlf_and_indentation() {
        let src = "[package]\r\n  version = \"0.1.0\"\r\nname = \"x\"\r\n";
        assert_eq!(
            set_package_version(src, "1.2.3").unwrap(),
            "[package]\r\n  version = \"1.2.3\"\r\nname = \"x\"\r\n"
        );
    }

    #[test]
    fn workspace_version_is_rejected() {
        let src = "[package]\nname = \"x\"\nversion.workspace = true\n";
        let err = set_package_version(src, "0.2.0").unwrap_err();
        assert!(err.contains("workspace"), "{err}");
    }

    #[test]
    fn missing_version_is_rejected() {
        let src = "[package]\nname = \"x\"\n[dependencies]\nversion = \"1\"\n";
        assert!(set_package_version(src, "0.2.0").is_err());
    }

    #[test]
    fn artifact_name_prefers_lib_name() {
        assert_eq!(artifact_name(CRATE).as_deref(), Some("git"));
    }

    #[test]
    fn artifact_name_falls_back_to_package_with_underscores() {
        let src = "[package]\nname = \"semantic-release\"\nnamespace = \"no\"\n";
        assert_eq!(artifact_name(src).as_deref(), Some("semantic_release"));
    }

    #[test]
    fn inline_workspace_version_is_rejected() {
        let src = "[package]\nname = \"x\"\nversion = { workspace = true }\n";
        let err = set_package_version(src, "0.2.0").unwrap_err();
        assert!(err.contains("workspace"), "{err}");
    }
}
