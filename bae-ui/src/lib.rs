//! bae-ui - Shared UI types and components for bae
//!
//! Contains display types, stores, and pure view components used by both
//! the desktop app and web demo.

pub mod components;
pub mod display_types;
pub mod floating_ui;
pub mod platform;
pub mod stores;
pub mod wasm_utils;

pub use components::*;
pub use display_types::*;
