pub fn remove_trailling_slash(string: &str) -> String {
    if let Some(end) = string.strip_suffix('/') {
        end.to_string()
    } else {
        string.to_string()
    }
}
