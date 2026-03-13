use super::*;

pub(super) fn parse_arguments(arguments: &str) -> Result<Value> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(trimmed).context("tool arguments must be valid JSON")
}

pub(super) fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("tool argument '{key}' is required"))
}

pub(super) fn optional_string<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

pub(super) fn optional_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

pub(super) fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

pub(super) fn optional_i64_array(args: &Value, key: &str) -> Option<Vec<i64>> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
}

pub(super) fn optional_string_array(args: &Value, key: &str) -> Option<Vec<String>> {
    args.get(key).and_then(Value::as_array).map(|values| {
        values
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    })
}

pub(super) fn required_string_array(args: &Value, key: &str) -> Result<Vec<String>> {
    let values = optional_string_array(args, key)
        .ok_or_else(|| anyhow!("tool argument '{key}' must be an array of strings"))?;
    if values.is_empty() {
        bail!("tool argument '{key}' must not be empty");
    }
    Ok(values)
}

pub(super) fn required_i64(args: &Value, key: &str) -> Result<i64> {
    args.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("tool argument '{key}' must be an integer"))
}

pub(super) fn is_sensitive_env_var(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD", "COOKIE", "SESSION"]
        .iter()
        .any(|fragment| upper.contains(fragment))
}

pub(super) fn truncate(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }

    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...(truncated)", &text[..end])
}
