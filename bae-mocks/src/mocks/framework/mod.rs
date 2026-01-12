//! Mock framework for Storybook-like component development
//!
//! Provides:
//! - ControlRegistry: Typed control bag with automatic URL sync
//! - Presets: Named state configurations for quick switching
//! - MockPanel: Auto-generated control panel UI with built-in viewport switching

mod panel;
mod preset;
mod registry;
mod viewport;

pub use panel::MockPanel;
pub use preset::Preset;
pub use registry::ControlRegistryBuilder;
// MockViewport is used by MockPanel internally
