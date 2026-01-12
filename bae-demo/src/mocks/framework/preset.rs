//! State presets for quick configuration switching

use super::registry::ControlValue;
use std::collections::HashMap;

/// A named preset with predefined control values
#[derive(Clone, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub values: HashMap<String, ControlValue>,
}

impl Preset {
    /// Create a new preset with the given name
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            values: HashMap::new(),
        }
    }

    /// Set a boolean value in this preset
    pub fn set_bool(mut self, key: &'static str, value: bool) -> Self {
        self.values
            .insert(key.to_string(), ControlValue::Bool(value));
        self
    }

    /// Set a string/enum value in this preset
    pub fn set_string(mut self, key: &'static str, value: &'static str) -> Self {
        self.values
            .insert(key.to_string(), ControlValue::String(value.to_string()));
        self
    }
}
