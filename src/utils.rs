use std::collections::BTreeMap;

pub fn remove_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Collects keys from the first map that are not present in the second map.
pub fn collect_missing_keys<'a>(
    first: &'a BTreeMap<String, String>,
    second: &'a BTreeMap<String, String>,
) -> Vec<&'a str> {
    first
        .iter()
        .filter(|(key, _)| !second.contains_key(key.as_str()))
        .map(|(key, _)| key.as_str())
        .collect()
}

/// Returns the first key that is missing or diffirent in the second map.
pub fn a_key_is_missing_or_diffirent<'a>(
    first: &'a BTreeMap<String, String>,
    second: &'a BTreeMap<String, String>,
) -> Option<&'a str> {
    for (key, value) in first.iter() {
        if let Some(second_value) = second.get(key) {
            if value != second_value {
                return Some(key);
            }
        } else {
            return Some(key);
        }
    }

    None
}
