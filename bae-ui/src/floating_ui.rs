//! Floating UI bindings for Dioxus
//!
//! Provides typed Rust interface to @floating-ui/dom for positioning floating elements
//! (dropdowns, tooltips, popovers) relative to anchor elements.
//!
//! The library is loaded via vendored scripts in Dioxus.toml.

use wasm_bindgen_x::prelude::*;
use wasm_bindgen_x::JsCast;

/// Placement options matching floating-ui's placement values
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Placement {
    #[default]
    Bottom,
    BottomStart,
    BottomEnd,
    Top,
    TopStart,
    TopEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

impl Placement {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::BottomStart => "bottom-start",
            Self::BottomEnd => "bottom-end",
            Self::Top => "top",
            Self::TopStart => "top-start",
            Self::TopEnd => "top-end",
            Self::Left => "left",
            Self::LeftStart => "left-start",
            Self::LeftEnd => "left-end",
            Self::Right => "right",
            Self::RightStart => "right-start",
            Self::RightEnd => "right-end",
        }
    }
}

/// Result of computePosition
#[derive(Debug, Clone, Copy, Default)]
pub struct ComputePositionResult {
    pub x: f64,
    pub y: f64,
}

/// Options for computePosition
#[derive(Debug, Clone, Default)]
pub struct ComputePositionOptions {
    pub placement: Placement,
    pub offset: Option<f64>,
    pub cross_axis_offset: Option<f64>,
    pub flip: bool,
    pub shift: bool,
}

/// Compute position of floating element relative to reference element.
///
/// Returns (x, y) coordinates to apply to the floating element's style.
pub async fn compute_position(
    reference: &web_sys_x::Element,
    floating: &web_sys_x::Element,
    options: ComputePositionOptions,
) -> Result<ComputePositionResult, JsValue> {
    let window = web_sys_x::window().ok_or("no window")?;
    let floating_ui = js_sys_x::Reflect::get(&window, &"FloatingUIDOM".into())?;

    // Build middleware array
    let middleware = js_sys_x::Array::new();

    if options.offset.is_some() || options.cross_axis_offset.is_some() {
        let offset_fn = js_sys_x::Reflect::get(&floating_ui, &"offset".into())?;
        if let Some(func) = offset_fn.dyn_ref::<js_sys_x::Function>() {
            let arg = if options.cross_axis_offset.is_some() {
                let obj = js_sys_x::Object::new();
                js_sys_x::Reflect::set(
                    &obj,
                    &"mainAxis".into(),
                    &JsValue::from_f64(options.offset.unwrap_or(0.0)),
                )?;
                js_sys_x::Reflect::set(
                    &obj,
                    &"crossAxis".into(),
                    &JsValue::from_f64(options.cross_axis_offset.unwrap_or(0.0)),
                )?;
                obj.into()
            } else {
                JsValue::from_f64(options.offset.unwrap_or(0.0))
            };
            let offset_middleware = func.call1(&JsValue::NULL, &arg)?;
            middleware.push(&offset_middleware);
        }
    }

    if options.flip {
        let flip_fn = js_sys_x::Reflect::get(&floating_ui, &"flip".into())?;
        if let Some(func) = flip_fn.dyn_ref::<js_sys_x::Function>() {
            let flip_middleware = func.call0(&JsValue::NULL)?;
            middleware.push(&flip_middleware);
        }
    }

    if options.shift {
        let shift_fn = js_sys_x::Reflect::get(&floating_ui, &"shift".into())?;
        if let Some(func) = shift_fn.dyn_ref::<js_sys_x::Function>() {
            let shift_middleware = func.call0(&JsValue::NULL)?;
            middleware.push(&shift_middleware);
        }
    }

    // Build options object
    let opts = js_sys_x::Object::new();
    js_sys_x::Reflect::set(
        &opts,
        &"placement".into(),
        &options.placement.as_str().into(),
    )?;
    js_sys_x::Reflect::set(&opts, &"middleware".into(), &middleware)?;

    // Call computePosition(reference, floating, options)
    let compute_fn = js_sys_x::Reflect::get(&floating_ui, &"computePosition".into())?;
    let func = compute_fn
        .dyn_ref::<js_sys_x::Function>()
        .ok_or("computePosition not a function")?;

    let promise = func
        .call3(&JsValue::NULL, reference, floating, &opts)?
        .dyn_into::<js_sys_x::Promise>()?;

    let result = wasm_bindgen_futures_x::JsFuture::from(promise).await?;

    let x = js_sys_x::Reflect::get(&result, &"x".into())?
        .as_f64()
        .unwrap_or(0.0);
    let y = js_sys_x::Reflect::get(&result, &"y".into())?
        .as_f64()
        .unwrap_or(0.0);

    Ok(ComputePositionResult { x, y })
}
