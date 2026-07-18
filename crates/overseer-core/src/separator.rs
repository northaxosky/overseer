//! Separator name validation shared by mod and plugin lists

pub(crate) fn validate_separator_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("name cannot be empty".to_owned());
    }
    if name.chars().any(char::is_control) {
        return Err("name cannot contain control characters".to_owned());
    }
    if name.contains(['/', '\\']) {
        return Err("name cannot contain path separators".to_owned());
    }
    if name.starts_with('#') || name.starts_with('*') {
        return Err("name cannot start with `#` or `*`".to_owned());
    }
    Ok(name.to_owned())
}
