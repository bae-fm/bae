//! URL state persistence for mock pages
//!
//! Provides a simple state string format: key=value,key=value
//! e.g. "phase=ExactLookup,error=1,loading=1"

/// Parse state string into key-value pairs
pub fn parse_state(state: &str) -> Vec<(String, String)> {
    if state.is_empty() {
        return Vec::new();
    }
    state
        .split(',')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.to_string();
            let value = parts.next().unwrap_or("").to_string();
            Some((key, value))
        })
        .collect()
}

/// Build state string from key-value pairs
pub fn build_state(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(",")
}

/// Get a value from parsed state
pub fn get_state_value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Get bool from state
pub fn get_state_bool(pairs: &[(String, String)], key: &str, default: bool) -> bool {
    get_state_value(pairs, key)
        .map(|v| v == "1" || v == "true")
        .unwrap_or(default)
}

/// Get enum from state using a parser function
pub fn get_state_enum<T>(
    pairs: &[(String, String)],
    key: &str,
    default: T,
    from_str: fn(&str) -> Option<T>,
) -> T {
    get_state_value(pairs, key)
        .and_then(from_str)
        .unwrap_or(default)
}

/// Builder to collect state changes and produce a state string
pub struct StateBuilder {
    pairs: Vec<(String, String)>,
}

impl StateBuilder {
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    pub fn set_bool(&mut self, key: &str, value: bool, default: bool) {
        if value != default {
            self.pairs
                .push((key.to_string(), if value { "1" } else { "0" }.to_string()));
        }
    }

    pub fn set_enum<T: std::fmt::Debug + PartialEq>(&mut self, key: &str, value: &T, default: &T) {
        if value != default {
            self.pairs.push((key.to_string(), format!("{:?}", value)));
        }
    }

    pub fn build(mut self) -> String {
        self.pairs.sort_by(|a, b| a.0.cmp(&b.0));
        build_state(&self.pairs)
    }

    pub fn build_option(self) -> Option<String> {
        let s = self.build();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

impl Default for StateBuilder {
    fn default() -> Self {
        Self::new()
    }
}
