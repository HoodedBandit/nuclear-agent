use std::{
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value};

const REDACTED_VALUE: &str = "[REDACTED]";
const SENSITIVE_VALUE_KEYS: &[&str] = &[
    "access_token",
    "refresh_token",
    "id_token",
    "api_key",
    "authorization",
    "password",
    "secret",
    "subject_token",
    "daemon_token",
    "token",
];
const TOKEN_PREFIXES: &[&str] = &[
    "sk-", "ghp_", "gho_", "ghu_", "ghs_", "glpat-", "xoxb-", "xoxp-",
];

pub fn validate_single_path_component(value: &str, label: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{label} must not be empty");
    }
    if trimmed == "." || trimmed == ".." {
        bail!("{label} must not contain traversal segments");
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        bail!("{label} must be a single path component");
    }

    let path = Path::new(trimmed);
    let mut components = path.components();
    let Some(component) = components.next() else {
        bail!("{label} must not be empty");
    };
    if components.next().is_some() {
        bail!("{label} must be a single path component");
    }
    match component {
        Component::Normal(_) => Ok(trimmed.to_string()),
        _ => bail!("{label} must be a normal path component"),
    }
}

pub fn validate_relative_path(path: &Path, label: &str) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        bail!("{label} must not be empty");
    }
    if path.is_absolute() {
        bail!("{label} must be relative");
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            _ => bail!("{label} must not contain traversal or absolute path components"),
        }
    }

    if normalized.as_os_str().is_empty() {
        bail!("{label} must not be empty");
    }
    Ok(normalized)
}

pub fn resolve_relative_path_within_root(
    root: &Path,
    relative: &Path,
    label: &str,
) -> Result<PathBuf> {
    let relative = validate_relative_path(relative, label)?;
    resolve_path_within_root(root, &relative, label)
}

pub fn resolve_path_within_root(root: &Path, candidate: &Path, label: &str) -> Result<PathBuf> {
    let root = resolve_existing_ancestor(root, "managed root")?;
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let resolved = resolve_existing_ancestor(&candidate, label)?;
    if !resolved.starts_with(&root) {
        bail!(
            "{label} '{}' escapes managed root '{}'",
            resolved.display(),
            root.display()
        );
    }
    Ok(resolved)
}

pub fn resolve_operator_path(path: &Path, label: &str) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        bail!("{label} must not be empty");
    }
    resolve_existing_ancestor(path, label)
}

pub fn resolve_path_from_existing_parent(path: &Path, label: &str) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        bail!("{label} must not be empty");
    }

    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "{label} '{}' could not be resolved from a parent directory",
            path.display()
        )
    })?;
    let parent = resolve_operator_path(parent, &format!("{label} parent directory"))?;
    resolve_path_within_root(&parent, path, label)
}

pub fn redact_sensitive_json_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    if is_sensitive_key(key) {
                        (key.clone(), Value::String(REDACTED_VALUE.to_string()))
                    } else {
                        (key.clone(), redact_sensitive_json_value(value))
                    }
                })
                .collect::<Map<String, Value>>(),
        ),
        Value::Array(entries) => Value::Array(
            entries
                .iter()
                .map(redact_sensitive_json_value)
                .collect::<Vec<_>>(),
        ),
        Value::String(text) => Value::String(redact_sensitive_text(text)),
        _ => value.clone(),
    }
}

pub fn redact_sensitive_text(text: &str) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }

    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return serde_json::to_string(&redact_sensitive_json_value(&value))
            .unwrap_or_else(|_| REDACTED_VALUE.to_string());
    }

    let mut redacted = text.to_string();
    redacted = redact_prefixed_secret(&redacted, "Bearer ");
    for key in SENSITIVE_VALUE_KEYS {
        redacted = redact_keyed_secret(&redacted, key, ':');
        redacted = redact_keyed_secret(&redacted, key, '=');
    }
    for prefix in TOKEN_PREFIXES {
        redacted = redact_prefixed_secret(&redacted, prefix);
    }
    redact_jwt_like_segments(&redacted)
}

fn resolve_existing_ancestor(path: &Path, label: &str) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .with_context(|| format!("failed to resolve current directory for {label}"))?
            .join(path)
    };
    let mut missing = Vec::<OsString>::new();
    let mut current = absolute.as_path();
    loop {
        if current.exists() {
            let mut resolved = normalize_canonical_path(
                fs::canonicalize(current)
                    .with_context(|| format!("failed to canonicalize {}", current.display()))?,
            );
            for component in missing.iter().rev() {
                resolved.push(component);
            }
            return Ok(resolved);
        }

        let name = current.file_name().ok_or_else(|| {
            anyhow!(
                "{label} '{}' could not be resolved from an existing ancestor",
                path.display()
            )
        })?;
        missing.push(name.to_os_string());
        current = current.parent().ok_or_else(|| {
            anyhow!(
                "{label} '{}' could not be resolved from an existing ancestor",
                path.display()
            )
        })?;
    }
}

fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{stripped}"));
        }
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }

    path
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    SENSITIVE_VALUE_KEYS.iter().any(|fragment| {
        normalized == *fragment
            || normalized.ends_with(&format!("_{fragment}"))
            || normalized.contains(fragment)
    })
}

fn redact_keyed_secret(input: &str, key: &str, separator: char) -> String {
    let lower = input.to_ascii_lowercase();
    let key_lower = key.to_ascii_lowercase();
    let mut result = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative_index) = lower[cursor..].find(&key_lower) {
        let key_index = cursor + relative_index;
        result.push_str(&input[cursor..key_index]);
        result.push_str(&input[key_index..key_index + key.len()]);

        let mut value_start = key_index + key.len();
        while let Some(ch) = input[value_start..].chars().next() {
            if ch.is_whitespace() || ch == '"' || ch == '\'' {
                result.push(ch);
                value_start += ch.len_utf8();
                continue;
            }
            break;
        }

        if !input[value_start..].starts_with(separator) {
            cursor = key_index + key.len();
            continue;
        }
        result.push(separator);
        value_start += separator.len_utf8();

        while let Some(ch) = input[value_start..].chars().next() {
            if ch.is_whitespace() || ch == '"' || ch == '\'' {
                result.push(ch);
                value_start += ch.len_utf8();
                continue;
            }
            break;
        }

        let value_end = find_secret_boundary(input, value_start);
        if value_end > value_start {
            result.push_str(REDACTED_VALUE);
            cursor = value_end;
        } else {
            cursor = value_start;
        }
    }

    result.push_str(&input[cursor..]);
    result
}

fn redact_prefixed_secret(input: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative_index) = input[cursor..].find(prefix) {
        let prefix_index = cursor + relative_index;
        result.push_str(&input[cursor..prefix_index]);
        result.push_str(prefix);
        let value_start = prefix_index + prefix.len();
        let value_end = find_secret_boundary(input, value_start);
        if value_end > value_start {
            result.push_str(REDACTED_VALUE);
            cursor = value_end;
        } else {
            cursor = value_start;
        }
    }

    result.push_str(&input[cursor..]);
    result
}

fn redact_jwt_like_segments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut segment_start = None;

    for (index, ch) in input.char_indices() {
        if is_secret_delimiter(ch) {
            if let Some(start) = segment_start.take() {
                result.push_str(&redact_segment(&input[start..index]));
            }
            result.push(ch);
        } else if segment_start.is_none() {
            segment_start = Some(index);
        }
    }

    if let Some(start) = segment_start {
        result.push_str(&redact_segment(&input[start..]));
    }

    result
}

fn redact_segment(segment: &str) -> String {
    if looks_like_jwt(segment) {
        REDACTED_VALUE.to_string()
    } else {
        segment.to_string()
    }
}

fn looks_like_jwt(segment: &str) -> bool {
    let parts = segment.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            part.len() >= 6
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        })
}

fn find_secret_boundary(input: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in input[start..].char_indices() {
        if is_secret_delimiter(ch) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn is_secret_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '"' | '\'' | ',' | '&' | ';' | ')' | '(' | ']' | '[' | '{' | '}' | '=' | ':'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(prefix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn single_path_component_rejects_traversal_and_separators() {
        assert!(validate_single_path_component("", "component").is_err());
        assert!(validate_single_path_component(".", "component").is_err());
        assert!(validate_single_path_component("..", "component").is_err());
        assert!(validate_single_path_component("plugin/id", "component").is_err());
        assert!(validate_single_path_component("plugin\\id", "component").is_err());
        assert!(validate_single_path_component("/tmp/plugin", "component").is_err());
        assert_eq!(
            validate_single_path_component("plugin-id", "component").unwrap(),
            "plugin-id"
        );
    }

    #[test]
    fn resolve_relative_path_within_root_rejects_escape() {
        let root = temp_dir("agent-core-safety-root");
        let error = resolve_relative_path_within_root(&root, Path::new("../outside"), "candidate")
            .unwrap_err();
        assert!(error.to_string().contains("traversal"));
    }

    #[test]
    fn resolve_path_within_root_accepts_non_existing_targets_under_root() {
        let root = temp_dir("agent-core-safety-root");
        let normalized_root = normalize_canonical_path(fs::canonicalize(&root).unwrap());
        let target = resolve_relative_path_within_root(
            &root,
            Path::new("nested/output/file.txt"),
            "candidate",
        )
        .unwrap();
        assert!(target.starts_with(&normalized_root));
        assert!(target.ends_with(Path::new("nested").join("output").join("file.txt")));
    }

    #[test]
    fn resolve_path_from_existing_parent_accepts_non_existing_targets() {
        let root = temp_dir("agent-core-safety-parent");
        let target = resolve_path_from_existing_parent(
            &root.join("nested").join("artifact.json"),
            "artifact path",
        )
        .unwrap();

        assert!(target.starts_with(normalize_canonical_path(fs::canonicalize(&root).unwrap())));
        assert!(target.ends_with(Path::new("nested").join("artifact.json")));
    }

    #[test]
    fn redact_sensitive_text_masks_plain_tokens() {
        let redacted = redact_sensitive_text(
            "authorization=Bearer sk-live-123456 refresh_token=refresh-secret jwt=eyJhbGciOiJIUzI1Ni.eyJzdWIiOiIxMjM0NTYifQ.signature",
        );
        assert!(!redacted.contains("sk-live-123456"));
        assert!(!redacted.contains("refresh-secret"));
        assert!(!redacted.contains("eyJhbGciOiJIUzI1Ni"));
        assert!(redacted.contains(REDACTED_VALUE));
    }

    #[test]
    fn redact_sensitive_json_value_masks_nested_secret_fields() {
        let value = serde_json::json!({
            "access_token": "secret-token",
            "error": {
                "message": "bad key sk-live-123456"
            },
            "nested": {
                "refresh_token": "refresh-secret"
            }
        });
        let redacted = redact_sensitive_json_value(&value);
        let serialized = serde_json::to_string(&redacted).unwrap();
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("refresh-secret"));
        assert!(!serialized.contains("sk-live-123456"));
        assert!(serialized.contains(REDACTED_VALUE));
    }
}
