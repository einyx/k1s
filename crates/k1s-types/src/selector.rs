//! Label and field selector parsing and matching

use std::collections::BTreeMap;

/// A parsed label selector requirement
#[derive(Debug, Clone, PartialEq)]
pub enum LabelRequirement {
    /// Key equals value (key=value or key==value)
    Equals(String, String),
    /// Key not equals value (key!=value)
    NotEquals(String, String),
    /// Key exists
    Exists(String),
    /// Key does not exist
    DoesNotExist(String),
    /// Key in set of values (key in (v1,v2,...))
    In(String, Vec<String>),
    /// Key not in set of values (key notin (v1,v2,...))
    NotIn(String, Vec<String>),
}

impl LabelRequirement {
    /// Check if labels match this requirement
    pub fn matches(&self, labels: &BTreeMap<String, String>) -> bool {
        match self {
            LabelRequirement::Equals(key, value) => {
                labels.get(key).map(|v| v == value).unwrap_or(false)
            }
            LabelRequirement::NotEquals(key, value) => {
                labels.get(key).map(|v| v != value).unwrap_or(true)
            }
            LabelRequirement::Exists(key) => labels.contains_key(key),
            LabelRequirement::DoesNotExist(key) => !labels.contains_key(key),
            LabelRequirement::In(key, values) => {
                labels.get(key).map(|v| values.contains(v)).unwrap_or(false)
            }
            LabelRequirement::NotIn(key, values) => {
                labels.get(key).map(|v| !values.contains(v)).unwrap_or(true)
            }
        }
    }
}

/// A parsed label selector consisting of multiple requirements
#[derive(Debug, Clone, Default)]
pub struct LabelSelector {
    pub requirements: Vec<LabelRequirement>,
}

impl LabelSelector {
    /// Create an empty selector that matches everything
    pub fn new() -> Self {
        Self { requirements: vec![] }
    }

    /// Parse a label selector string
    /// Supports: key=value, key==value, key!=value, key, !key, key in (v1,v2), key notin (v1,v2)
    pub fn parse(selector: &str) -> Result<Self, String> {
        if selector.is_empty() {
            return Ok(Self::new());
        }

        let mut requirements = Vec::new();

        for part in split_selector(selector) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            let req = parse_requirement(part)?;
            requirements.push(req);
        }

        Ok(Self { requirements })
    }

    /// Check if labels match all requirements
    pub fn matches(&self, labels: &BTreeMap<String, String>) -> bool {
        self.requirements.iter().all(|req| req.matches(labels))
    }

    /// Check if the selector is empty (matches everything)
    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty()
    }
}

/// Split selector by commas, respecting parentheses
fn split_selector(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;

    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    if start < s.len() {
        parts.push(&s[start..]);
    }

    parts
}

/// Parse a single requirement
fn parse_requirement(s: &str) -> Result<LabelRequirement, String> {
    let s = s.trim();

    // Check for "in" or "notin" operators
    if let Some(idx) = s.find(" in (") {
        let key = s[..idx].trim();
        let values_str = &s[idx + 5..];
        let values = parse_values(values_str)?;
        return Ok(LabelRequirement::In(key.to_string(), values));
    }

    if let Some(idx) = s.find(" notin (") {
        let key = s[..idx].trim();
        let values_str = &s[idx + 8..];
        let values = parse_values(values_str)?;
        return Ok(LabelRequirement::NotIn(key.to_string(), values));
    }

    // Check for != operator
    if let Some(idx) = s.find("!=") {
        let key = s[..idx].trim();
        let value = s[idx + 2..].trim();
        return Ok(LabelRequirement::NotEquals(key.to_string(), value.to_string()));
    }

    // Check for == operator
    if let Some(idx) = s.find("==") {
        let key = s[..idx].trim();
        let value = s[idx + 2..].trim();
        return Ok(LabelRequirement::Equals(key.to_string(), value.to_string()));
    }

    // Check for = operator
    if let Some(idx) = s.find('=') {
        let key = s[..idx].trim();
        let value = s[idx + 1..].trim();
        return Ok(LabelRequirement::Equals(key.to_string(), value.to_string()));
    }

    // Check for !key (does not exist)
    if s.starts_with('!') {
        let key = s[1..].trim();
        return Ok(LabelRequirement::DoesNotExist(key.to_string()));
    }

    // Just a key (exists)
    Ok(LabelRequirement::Exists(s.to_string()))
}

/// Parse values from "v1,v2,...)" format
fn parse_values(s: &str) -> Result<Vec<String>, String> {
    let s = s.trim();
    let s = s.trim_end_matches(')');

    Ok(s.split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect())
}

/// A parsed field selector for filtering by resource fields
#[derive(Debug, Clone, Default)]
pub struct FieldSelector {
    pub requirements: Vec<FieldRequirement>,
}

/// A single field selector requirement
#[derive(Debug, Clone, PartialEq)]
pub enum FieldRequirement {
    /// Field equals value
    Equals(String, String),
    /// Field not equals value
    NotEquals(String, String),
}

impl FieldSelector {
    /// Create an empty selector that matches everything
    pub fn new() -> Self {
        Self { requirements: vec![] }
    }

    /// Parse a field selector string
    /// Supports: field=value, field==value, field!=value
    pub fn parse(selector: &str) -> Result<Self, String> {
        if selector.is_empty() {
            return Ok(Self::new());
        }

        let mut requirements = Vec::new();

        for part in selector.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            let req = parse_field_requirement(part)?;
            requirements.push(req);
        }

        Ok(Self { requirements })
    }

    /// Check if the selector is empty (matches everything)
    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty()
    }

    /// Match against a JSON value
    pub fn matches_json(&self, value: &serde_json::Value) -> bool {
        self.requirements.iter().all(|req| req.matches_json(value))
    }
}

impl FieldRequirement {
    /// Match against a JSON value by extracting the field path
    pub fn matches_json(&self, value: &serde_json::Value) -> bool {
        match self {
            FieldRequirement::Equals(path, expected) => {
                get_json_field(value, path)
                    .map(|v| json_value_equals_string(v, expected))
                    .unwrap_or(false)
            }
            FieldRequirement::NotEquals(path, expected) => {
                get_json_field(value, path)
                    .map(|v| !json_value_equals_string(v, expected))
                    .unwrap_or(true)
            }
        }
    }
}

/// Parse a single field requirement
fn parse_field_requirement(s: &str) -> Result<FieldRequirement, String> {
    let s = s.trim();

    // Check for != operator
    if let Some(idx) = s.find("!=") {
        let field = s[..idx].trim();
        let value = s[idx + 2..].trim();
        return Ok(FieldRequirement::NotEquals(field.to_string(), value.to_string()));
    }

    // Check for == operator
    if let Some(idx) = s.find("==") {
        let field = s[..idx].trim();
        let value = s[idx + 2..].trim();
        return Ok(FieldRequirement::Equals(field.to_string(), value.to_string()));
    }

    // Check for = operator
    if let Some(idx) = s.find('=') {
        let field = s[..idx].trim();
        let value = s[idx + 1..].trim();
        return Ok(FieldRequirement::Equals(field.to_string(), value.to_string()));
    }

    Err(format!("Invalid field selector: {}", s))
}

/// Get a nested JSON field by dot-separated path
fn get_json_field<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

/// Check if a JSON value equals a string (handles string and other types)
fn json_value_equals_string(value: &serde_json::Value, expected: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s == expected,
        serde_json::Value::Number(n) => n.to_string() == expected,
        serde_json::Value::Bool(b) => b.to_string() == expected,
        serde_json::Value::Null => expected == "null" || expected.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equals() {
        let sel = LabelSelector::parse("app=nginx").unwrap();

        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "nginx".to_string());
        assert!(sel.matches(&labels));

        labels.insert("app".to_string(), "redis".to_string());
        assert!(!sel.matches(&labels));
    }

    #[test]
    fn test_not_equals() {
        let sel = LabelSelector::parse("app!=nginx").unwrap();

        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "redis".to_string());
        assert!(sel.matches(&labels));

        labels.insert("app".to_string(), "nginx".to_string());
        assert!(!sel.matches(&labels));
    }

    #[test]
    fn test_exists() {
        let sel = LabelSelector::parse("app").unwrap();

        let mut labels = BTreeMap::new();
        assert!(!sel.matches(&labels));

        labels.insert("app".to_string(), "anything".to_string());
        assert!(sel.matches(&labels));
    }

    #[test]
    fn test_does_not_exist() {
        let sel = LabelSelector::parse("!app").unwrap();

        let mut labels = BTreeMap::new();
        assert!(sel.matches(&labels));

        labels.insert("app".to_string(), "anything".to_string());
        assert!(!sel.matches(&labels));
    }

    #[test]
    fn test_in() {
        let sel = LabelSelector::parse("env in (prod,staging)").unwrap();

        let mut labels = BTreeMap::new();
        labels.insert("env".to_string(), "prod".to_string());
        assert!(sel.matches(&labels));

        labels.insert("env".to_string(), "dev".to_string());
        assert!(!sel.matches(&labels));
    }

    #[test]
    fn test_multiple() {
        let sel = LabelSelector::parse("app=nginx,env!=dev").unwrap();

        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "nginx".to_string());
        labels.insert("env".to_string(), "prod".to_string());
        assert!(sel.matches(&labels));

        labels.insert("env".to_string(), "dev".to_string());
        assert!(!sel.matches(&labels));
    }

    #[test]
    fn test_field_selector_name() {
        use serde_json::json;

        let sel = FieldSelector::parse("metadata.name=nginx").unwrap();

        let pod = json!({
            "metadata": {
                "name": "nginx",
                "namespace": "default"
            }
        });
        assert!(sel.matches_json(&pod));

        let pod2 = json!({
            "metadata": {
                "name": "redis",
                "namespace": "default"
            }
        });
        assert!(!sel.matches_json(&pod2));
    }

    #[test]
    fn test_field_selector_status_phase() {
        use serde_json::json;

        let sel = FieldSelector::parse("status.phase=Running").unwrap();

        let pod = json!({
            "metadata": {"name": "test"},
            "status": {"phase": "Running"}
        });
        assert!(sel.matches_json(&pod));

        let pod2 = json!({
            "metadata": {"name": "test"},
            "status": {"phase": "Pending"}
        });
        assert!(!sel.matches_json(&pod2));
    }

    #[test]
    fn test_field_selector_multiple() {
        use serde_json::json;

        let sel = FieldSelector::parse("metadata.namespace=default,status.phase!=Failed").unwrap();

        let pod = json!({
            "metadata": {"name": "test", "namespace": "default"},
            "status": {"phase": "Running"}
        });
        assert!(sel.matches_json(&pod));

        let pod2 = json!({
            "metadata": {"name": "test", "namespace": "default"},
            "status": {"phase": "Failed"}
        });
        assert!(!sel.matches_json(&pod2));
    }
}
