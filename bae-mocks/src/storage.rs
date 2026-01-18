//! Local storage helpers

pub fn get_storage() -> Option<web_sys_x::Storage> {
    web_sys_x::window().and_then(|w| w.local_storage().ok().flatten())
}

pub fn get_string(key: &str) -> Option<String> {
    get_storage().and_then(|s| s.get_item(key).ok().flatten())
}

pub fn set_string(key: &str, value: &str) {
    if let Some(storage) = get_storage() {
        let _ = storage.set_item(key, value);
    }
}

pub fn get_bool(key: &str) -> Option<bool> {
    get_string(key).map(|v| v == "true")
}

pub fn set_bool(key: &str, value: bool) {
    set_string(key, if value { "true" } else { "false" });
}

pub fn get_parsed<T: std::str::FromStr>(key: &str) -> Option<T> {
    get_string(key).and_then(|v| v.parse().ok())
}

pub fn set_display<T: std::fmt::Display>(key: &str, value: T) {
    set_string(key, &value.to_string());
}
