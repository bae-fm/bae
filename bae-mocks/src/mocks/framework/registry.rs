//! Control registry for typed control management with URL sync

use super::preset::Preset;
use crate::mocks::url_state::{parse_state, StateBuilder};
use crate::Route;
use dioxus::prelude::*;
use std::collections::HashMap;

/// Value stored in the control registry
#[derive(Clone, Debug, PartialEq)]
pub enum ControlValue {
    Bool(bool),
    String(String),
}

/// Definition of a control with metadata
#[derive(Clone, PartialEq)]
pub struct ControlDef {
    pub key: &'static str,
    pub label: &'static str,
    pub default: ControlValue,
    pub doc: Option<&'static str>,
    pub enum_options: Option<Vec<(&'static str, &'static str)>>, // (value, label) for enums
}

/// Builder for creating a ControlRegistry
pub struct ControlRegistryBuilder {
    controls: Vec<ControlDef>,
    presets: Vec<Preset>,
}

impl ControlRegistryBuilder {
    pub fn new() -> Self {
        Self {
            controls: Vec::new(),
            presets: Vec::new(),
        }
    }

    /// Add a boolean control
    pub fn bool_control(mut self, key: &'static str, label: &'static str, default: bool) -> Self {
        self.controls.push(ControlDef {
            key,
            label,
            default: ControlValue::Bool(default),
            doc: None,
            enum_options: None,
        });
        self
    }

    /// Add an enum control (represented as string internally)
    pub fn enum_control(
        mut self,
        key: &'static str,
        label: &'static str,
        default: &'static str,
        options: Vec<(&'static str, &'static str)>,
    ) -> Self {
        self.controls.push(ControlDef {
            key,
            label,
            default: ControlValue::String(default.to_string()),
            doc: None,
            enum_options: Some(options),
        });
        self
    }

    /// Add documentation to the last control
    pub fn doc(mut self, doc: &'static str) -> Self {
        if let Some(last) = self.controls.last_mut() {
            last.doc = Some(doc);
        }
        self
    }

    /// Add state presets
    pub fn with_presets(mut self, presets: Vec<Preset>) -> Self {
        self.presets = presets;
        self
    }

    /// Build the registry - must be called inside a component (uses hooks)
    pub fn build(self, initial_state: Option<String>) -> ControlRegistry {
        let state_pairs = initial_state
            .as_deref()
            .map(parse_state)
            .unwrap_or_default();

        let mut values: HashMap<&'static str, Signal<ControlValue>> = HashMap::new();

        for def in &self.controls {
            let initial = match &def.default {
                ControlValue::Bool(default) => {
                    let parsed = state_pairs
                        .iter()
                        .find(|(k, _)| k == def.key)
                        .map(|(_, v)| v == "1" || v == "true")
                        .unwrap_or(*default);
                    ControlValue::Bool(parsed)
                }
                ControlValue::String(default) => {
                    let parsed = state_pairs
                        .iter()
                        .find(|(k, _)| k == def.key)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| default.clone());
                    ControlValue::String(parsed)
                }
            };
            // Use use_signal to properly hook into Dioxus reactive system
            let signal = use_signal(|| initial);
            values.insert(def.key, signal);
        }

        ControlRegistry {
            controls: self.controls,
            values,
            presets: self.presets,
        }
    }
}

impl Default for ControlRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry holding all controls and their current values
#[derive(Clone, PartialEq)]
pub struct ControlRegistry {
    pub controls: Vec<ControlDef>,
    pub values: HashMap<&'static str, Signal<ControlValue>>,
    pub presets: Vec<Preset>,
}

impl ControlRegistry {
    /// Get a boolean value (reads signal, creating subscription)
    pub fn get_bool(&self, key: &'static str) -> bool {
        self.values
            .get(key)
            .map(|s| match &*s.read() {
                ControlValue::Bool(b) => *b,
                _ => false,
            })
            .unwrap_or(false)
    }

    /// Get a string value (reads signal, creating subscription)
    pub fn get_string(&self, key: &'static str) -> String {
        self.values
            .get(key)
            .map(|s| match &*s.read() {
                ControlValue::String(s) => s.clone(),
                _ => String::new(),
            })
            .unwrap_or_default()
    }

    /// Set a boolean value
    pub fn set_bool(&self, key: &'static str, value: bool) {
        if let Some(mut signal) = self.values.get(key).copied() {
            signal.set(ControlValue::Bool(value));
        }
    }

    /// Set a string value (for enums)
    pub fn set_string(&self, key: &'static str, value: String) {
        if let Some(mut signal) = self.values.get(key).copied() {
            signal.set(ControlValue::String(value));
        }
    }

    /// Apply a preset
    pub fn apply_preset(&self, preset: &Preset) {
        for (key, value) in &preset.values {
            if let Some(mut signal) = self.values.get(key.as_str()).copied() {
                signal.set(value.clone());
            }
        }
    }

    /// Build URL state string from current values
    pub fn build_state(&self) -> Option<String> {
        let mut builder = StateBuilder::new();

        for def in &self.controls {
            if let Some(signal) = self.values.get(def.key) {
                let value = signal.read();
                match (&*value, &def.default) {
                    (ControlValue::Bool(v), ControlValue::Bool(default)) => {
                        builder.set_bool(def.key, *v, *default);
                    }
                    (ControlValue::String(v), ControlValue::String(default)) => {
                        if v != default {
                            builder.set_string(def.key, v);
                        }
                    }
                    _ => {}
                }
            }
        }

        builder.build_option()
    }

    /// Create a URL sync effect for FolderImport mock
    pub fn use_url_sync_folder_import(&self) {
        let registry = self.clone();
        let mut is_mounted = use_signal(|| false);

        use_effect(move || {
            // Read all values to subscribe to changes
            for signal in registry.values.values() {
                let _ = signal.read();
            }

            if !*is_mounted.peek() {
                is_mounted.set(true);
                return;
            }

            navigator().replace(Route::MockFolderImport {
                state: registry.build_state(),
            });
        });
    }

    /// Create a URL sync effect for AlbumDetail mock
    pub fn use_url_sync_album_detail(&self) {
        let registry = self.clone();
        let mut is_mounted = use_signal(|| false);

        use_effect(move || {
            // Read all values to subscribe to changes
            for signal in registry.values.values() {
                let _ = signal.read();
            }

            if !*is_mounted.peek() {
                is_mounted.set(true);
                return;
            }

            navigator().replace(Route::MockAlbumDetail {
                state: registry.build_state(),
            });
        });
    }

    /// Create a URL sync effect for Library mock
    pub fn use_url_sync_library(&self) {
        let registry = self.clone();
        let mut is_mounted = use_signal(|| false);

        use_effect(move || {
            // Read all values to subscribe to changes
            for signal in registry.values.values() {
                let _ = signal.read();
            }

            if !*is_mounted.peek() {
                is_mounted.set(true);
                return;
            }

            navigator().replace(Route::MockLibrary {
                state: registry.build_state(),
            });
        });
    }
}
