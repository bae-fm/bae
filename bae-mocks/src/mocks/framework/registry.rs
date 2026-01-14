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
    Int(i32),
}

/// Definition of a control with metadata
#[derive(Clone, PartialEq)]
pub struct ControlDef {
    pub key: &'static str,
    pub label: &'static str,
    pub default: ControlValue,
    pub doc: Option<&'static str>,
    pub enum_options: Option<Vec<(&'static str, &'static str)>>, // (value, label) for enums
    pub int_range: Option<(i32, Option<i32>)>,                   // (min, max) for int controls
    /// Conditions that must all be true for this control to be visible.
    /// Each tuple is (control_key, required_value) - the referenced control must have that value.
    pub visible_when: Vec<(&'static str, &'static str)>,
    /// If true, enum controls render as dropdowns inline with bool controls
    pub inline: bool,
}

impl ControlDef {
    /// Check if this control should be visible given the current registry state.
    /// Returns true if all `visible_when` conditions are met (or if there are none).
    pub fn is_visible(&self, registry: &ControlRegistry) -> bool {
        self.visible_when
            .iter()
            .all(|(key, value)| registry.get_string(key) == *value)
    }
}

/// Definition of an action button (not stored in URL params)
#[derive(Clone)]
pub struct ActionDef {
    pub label: &'static str,
    pub callback: Callback<()>,
}

/// Builder for creating a ControlRegistry
pub struct ControlRegistryBuilder {
    controls: Vec<ControlDef>,
    actions: Vec<ActionDef>,
    presets: Vec<Preset>,
}

impl ControlRegistryBuilder {
    pub fn new() -> Self {
        Self {
            controls: Vec::new(),
            actions: Vec::new(),
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
            int_range: None,
            visible_when: Vec::new(),
            inline: false,
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
            int_range: None,
            visible_when: Vec::new(),
            inline: false,
        });
        self
    }

    /// Add an integer control
    pub fn int_control(
        mut self,
        key: &'static str,
        label: &'static str,
        default: i32,
        min: i32,
        max: Option<i32>,
    ) -> Self {
        self.controls.push(ControlDef {
            key,
            label,
            default: ControlValue::Int(default),
            doc: None,
            enum_options: None,
            int_range: Some((min, max)),
            visible_when: Vec::new(),
            inline: false,
        });
        self
    }

    /// Add a free-form string control
    pub fn string_control(mut self, key: &'static str, label: &'static str, default: &str) -> Self {
        self.controls.push(ControlDef {
            key,
            label,
            default: ControlValue::String(default.to_string()),
            doc: None,
            enum_options: None,
            int_range: None,
            visible_when: Vec::new(),
            inline: false,
        });
        self
    }

    /// Mark the last control as inline (enum controls render as dropdowns with bool controls)
    pub fn inline(mut self) -> Self {
        if let Some(last) = self.controls.last_mut() {
            last.inline = true;
        }
        self
    }

    /// Add documentation to the last control
    pub fn doc(mut self, doc: &'static str) -> Self {
        if let Some(last) = self.controls.last_mut() {
            last.doc = Some(doc);
        }
        self
    }

    /// Add a visibility condition to the last control.
    /// The control will only be visible when the referenced control has the specified value.
    /// Multiple calls create an AND condition (all must match).
    pub fn visible_when(mut self, key: &'static str, value: &'static str) -> Self {
        if let Some(last) = self.controls.last_mut() {
            last.visible_when.push((key, value));
        }
        self
    }

    /// Add an action button (not stored in URL params)
    pub fn action(mut self, label: &'static str, callback: Callback<()>) -> Self {
        self.actions.push(ActionDef { label, callback });
        self
    }

    /// Add state presets
    pub fn with_presets(mut self, presets: Vec<Preset>) -> Self {
        self.presets = presets;
        self
    }

    /// Build the registry - must be called inside a component (uses hooks)
    pub fn build(self, initial_state: Option<String>) -> ControlRegistry {
        // Validate all visible_when conditions reference valid keys and values
        for control in &self.controls {
            for (ref_key, ref_value) in &control.visible_when {
                let ref_control = self.controls.iter().find(|c| c.key == *ref_key);
                match ref_control {
                    None => panic!(
                        "visible_when on '{}' references unknown control '{}'",
                        control.key, ref_key
                    ),
                    Some(ref_ctrl) => {
                        if let Some(options) = &ref_ctrl.enum_options {
                            if !options.iter().any(|(v, _)| v == ref_value) {
                                panic!(
                                    "visible_when on '{}' references invalid value '{}' for control '{}'. Valid values: {:?}",
                                    control.key, ref_value, ref_key,
                                    options.iter().map(|(v, _)| *v).collect::<Vec<_>>()
                                );
                            }
                        }
                    }
                }
            }
        }

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
                ControlValue::Int(default) => {
                    let parsed = state_pairs
                        .iter()
                        .find(|(k, _)| k == def.key)
                        .and_then(|(_, v)| v.parse().ok())
                        .unwrap_or(*default);
                    ControlValue::Int(parsed)
                }
            };
            // Use use_signal to properly hook into Dioxus reactive system
            let signal = use_signal(|| initial);
            values.insert(def.key, signal);
        }

        ControlRegistry {
            controls: self.controls,
            actions: self.actions,
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
#[derive(Clone)]
pub struct ControlRegistry {
    pub controls: Vec<ControlDef>,
    pub actions: Vec<ActionDef>,
    pub values: HashMap<&'static str, Signal<ControlValue>>,
    pub presets: Vec<Preset>,
}

impl PartialEq for ControlRegistry {
    fn eq(&self, other: &Self) -> bool {
        self.controls == other.controls
            && self.values == other.values
            && self.presets == other.presets
            && self.actions.len() == other.actions.len()
    }
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

    /// Get an integer value (reads signal, creating subscription)
    pub fn get_int(&self, key: &'static str) -> i32 {
        self.values
            .get(key)
            .map(|s| match &*s.read() {
                ControlValue::Int(i) => *i,
                _ => 0,
            })
            .unwrap_or(0)
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

    /// Set an integer value
    pub fn set_int(&self, key: &'static str, value: i32) {
        if let Some(mut signal) = self.values.get(key).copied() {
            signal.set(ControlValue::Int(value));
        }
    }

    /// Apply a preset - resets all controls to defaults, then applies preset values
    pub fn apply_preset(&self, preset: &Preset) {
        // First, reset all controls to their defaults
        for control in &self.controls {
            if let Some(mut signal) = self.values.get(control.key).copied() {
                signal.set(control.default.clone());
            }
        }
        // Then apply the preset's specific values
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
                    (ControlValue::Int(v), ControlValue::Int(default)) => {
                        if v != default {
                            builder.set_string(def.key, &v.to_string());
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
