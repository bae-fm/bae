//! Reading a rendered board the way the shots' platforms read theirs: Chrome's
//! accessibility tree becomes an element list in reading order, in the shape
//! the shot harnesses write (`role`, `native`, `label`, `value`, `bounds`,
//! `enabled`, `selected`, `text`, `parent`, `focused`, `placeholder`).
//!
//! The tree is read in two parts. [`build`] turns raw nodes into elements and
//! needs no browser: roles, words, folding a control's text into its name,
//! skipping ignored nodes. [`elements`] gathers the raw nodes from a page:
//! the accessibility tree, and for each node its box, from its DOM node (a text
//! node's box is the range of its words, not its parent's), and for text its
//! computed font.

use chromiumoxide::cdp::browser_protocol::accessibility::{
    AxNode, AxNodeId, AxPropertyName, EnableParams, GetFullAxTreeParams,
};
use chromiumoxide::cdp::browser_protocol::dom::ResolveNodeParams;
use chromiumoxide::cdp::js_protocol::runtime::CallFunctionOnParams;
use chromiumoxide::Page;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    #[error("Chrome failed: {0}")]
    Protocol(String),
    #[error("the accessibility tree has no node {0}")]
    NoNode(String),
}

fn protocol(error: impl std::fmt::Display) -> ReadError {
    ReadError::Protocol(error.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Role {
    Text,
    Button,
    Toggle,
    Radio,
    Field,
    Popup,
    Tab,
    Link,
    Image,
    Slider,
    Progress,
    Other,
}

impl Role {
    /// The shots' word for a Chrome role.
    fn of(chrome: &str) -> Self {
        match chrome {
            "button" => Self::Button,
            "switch" | "checkbox" => Self::Toggle,
            "radio" => Self::Radio,
            "textbox" | "searchbox" => Self::Field,
            "combobox" | "PopUpButton" | "listbox" => Self::Popup,
            "tab" => Self::Tab,
            "link" => Self::Link,
            "img" | "image" => Self::Image,
            "slider" => Self::Slider,
            "progressbar" => Self::Progress,
            "StaticText" => Self::Text,
            _ => Self::Other,
        }
    }

    /// Roles whose name is their words, which the text inside them repeats.
    fn is_control(self) -> bool {
        !matches!(self, Self::Text | Self::Image | Self::Other)
    }
}

/// Chrome roles that only hold other nodes and say nothing of their own.
const STRUCTURE: [&str; 6] =
    ["generic", "none", "presentation", "RootWebArea", "InlineTextBox", "LineBreak"];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Bounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// A text element's style: size in points, weight 100-900, colour as sRGB
/// `#RRGGBBAA`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct TextStyle {
    pub(crate) size: f64,
    pub(crate) weight: u32,
    pub(crate) color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Element {
    pub(crate) role: Role,
    /// Chrome's own role string.
    pub(crate) native: String,
    pub(crate) label: Option<String>,
    pub(crate) value: Option<String>,
    pub(crate) bounds: Bounds,
    pub(crate) enabled: bool,
    pub(crate) selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<TextStyle>,
    /// The index in the list of the enclosing element.
    pub(crate) parent: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) focused: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) placeholder: Option<String>,
}

/// What the browser holds about one node of the accessibility tree.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Raw {
    pub(crate) id: String,
    pub(crate) parent: Option<String>,
    pub(crate) children: Vec<String>,
    pub(crate) ignored: bool,
    pub(crate) chrome_role: String,
    pub(crate) name: Option<String>,
    pub(crate) value: Option<String>,
    /// `true`, `false` or `mixed`.
    pub(crate) checked: Option<String>,
    pub(crate) selected: Option<bool>,
    pub(crate) disabled: bool,
    pub(crate) focused: bool,
    pub(crate) bounds: Option<Bounds>,
    pub(crate) text: Option<TextStyle>,
    pub(crate) placeholder: Option<String>,
}

/// Words: composed Unicode, whitespace runs collapsed to one space, trimmed.
pub(crate) fn words(text: &str) -> String {
    text.nfc().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn nonempty(text: Option<&str>) -> Option<String> {
    text.map(words).filter(|w| !w.is_empty())
}

/// The elements of `raw`, in reading order (the tree's order from its first
/// node). Ignored nodes (hidden, `aria-hidden`, presentational) are left out,
/// their visible children kept; a text inside a named control is the
/// control's name, not an element of its own.
pub(crate) fn build(raw: &[Raw]) -> Result<Vec<Element>, ReadError> {
    let by_id: HashMap<&str, &Raw> = raw.iter().map(|n| (n.id.as_str(), n)).collect();
    let Some(first) = raw.first() else { return Ok(Vec::new()) };
    let mut elements = Vec::new();
    // (node, index of the nearest kept ancestor, a named control above it)
    let mut pending = vec![(first, None, false)];
    while let Some((node, parent, folded)) = pending.pop() {
        let role = Role::of(&node.chrome_role);
        let label = nonempty(node.name.as_deref()).filter(|_| role != Role::Other);
        let kept = !node.ignored && !STRUCTURE.contains(&node.chrome_role.as_str());
        let inside_control = folded;
        let mut next_parent = parent;
        let mut next_folded = folded;
        if kept && !(role == Role::Text && inside_control) {
            if let Some(bounds) = node.bounds {
                next_parent = Some(elements.len());
                elements.push(element(node, role, label.clone(), bounds, parent));
            }
        }
        if kept && role.is_control() && label.is_some() {
            next_folded = true;
        }
        for child in node.children.iter().rev() {
            let child =
                by_id.get(child.as_str()).ok_or_else(|| ReadError::NoNode(child.clone()))?;
            pending.push((child, next_parent, next_folded));
        }
    }
    Ok(elements)
}

fn element(
    node: &Raw,
    role: Role,
    label: Option<String>,
    bounds: Bounds,
    parent: Option<usize>,
) -> Element {
    let value = match role {
        Role::Toggle => node.checked.as_deref().map(|c| match c {
            "true" => "on".to_string(),
            "false" => "off".to_string(),
            other => other.to_string(),
        }),
        Role::Field => Some(node.value.clone().unwrap_or_default()),
        Role::Popup | Role::Slider | Role::Progress => node.value.clone(),
        _ => None,
    };
    let selected = match role {
        Role::Radio => node.checked.as_deref().map(|c| c == "true"),
        _ => node.selected,
    };
    Element {
        role,
        native: node.chrome_role.clone(),
        label,
        value,
        bounds,
        enabled: !node.disabled,
        selected,
        text: if role == Role::Text { node.text.clone() } else { None },
        parent,
        focused: node.focused.then_some(true),
        placeholder: if role == Role::Field { nonempty(node.placeholder.as_deref()) } else { None },
    }
}

/// What a DOM node reports about its box and its font.
#[derive(Debug, Deserialize)]
struct Geometry {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    size: Option<f64>,
    weight: Option<u32>,
    color: Option<String>,
    placeholder: Option<String>,
}

const GEOMETRY: &str = r#"function() {
  const isText = this.nodeType === 3;
  let rect;
  if (isText) {
    const range = document.createRange();
    range.selectNodeContents(this);
    rect = range.getBoundingClientRect();
  } else if (this.getBoundingClientRect) {
    rect = this.getBoundingClientRect();
  } else {
    return null;
  }
  const element = isText ? this.parentElement : this;
  const style = element ? getComputedStyle(element) : null;
  return {
    x: rect.x, y: rect.y, width: rect.width, height: rect.height,
    size: style ? parseFloat(style.fontSize) : null,
    weight: style ? parseInt(style.fontWeight, 10) : null,
    color: style ? style.color : null,
    placeholder: !isText && typeof this.placeholder === 'string' && this.placeholder
      ? this.placeholder : null,
  };
}"#;

/// `rgb(r, g, b)` or `rgba(r, g, b, a)` as `#RRGGBBAA`; anything else is not
/// guessed.
fn srgb(css: &str) -> Option<String> {
    let inner =
        css.strip_prefix("rgba(").or_else(|| css.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let channel = |p: &str| p.parse::<f64>().ok().map(|v| v.round().clamp(0.0, 255.0) as u8);
    let (r, g, b) = (channel(parts[0])?, channel(parts[1])?, channel(parts[2])?);
    let a = match parts.get(3) {
        Some(alpha) => (alpha.parse::<f64>().ok()?.clamp(0.0, 1.0) * 255.0).round() as u8,
        None => 255,
    };
    Some(format!("#{r:02X}{g:02X}{b:02X}{a:02X}"))
}

fn string(
    value: &Option<chromiumoxide::cdp::browser_protocol::accessibility::AxValue>,
) -> Option<String> {
    match value.as_ref()?.value.as_ref()? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn raw(node: &AxNode) -> Raw {
    let property = |name: AxPropertyName| {
        node.properties
            .iter()
            .flatten()
            .find(|p| p.name == name)
            .map(|p| string(&Some(p.value.clone())))
    };
    Raw {
        id: node.node_id.inner().clone(),
        parent: node.parent_id.as_ref().map(|p| p.inner().clone()),
        children: node.child_ids.iter().flatten().map(|c: &AxNodeId| c.inner().clone()).collect(),
        ignored: node.ignored,
        chrome_role: string(&node.role).unwrap_or_default(),
        name: string(&node.name),
        value: string(&node.value),
        checked: property(AxPropertyName::Checked).flatten(),
        selected: property(AxPropertyName::Selected).flatten().map(|s| s == "true"),
        disabled: property(AxPropertyName::Disabled).flatten().is_some_and(|d| d == "true"),
        focused: property(AxPropertyName::Focused).flatten().is_some_and(|f| f == "true"),
        ..Raw::default()
    }
}

/// The page's elements in reading order.
pub(crate) async fn elements(page: &Page) -> Result<Vec<Element>, ReadError> {
    page.execute(EnableParams::default()).await.map_err(protocol)?;
    let tree = page.execute(GetFullAxTreeParams::default()).await.map_err(protocol)?;
    let mut nodes = Vec::new();
    for node in &tree.result.nodes {
        let mut raw = raw(node);
        if !raw.ignored && !STRUCTURE.contains(&raw.chrome_role.as_str()) {
            if let Some(backend) = node.backend_dom_node_id {
                let resolved = page
                    .execute(ResolveNodeParams::builder().backend_node_id(backend).build())
                    .await
                    .map_err(protocol)?;
                if let Some(object_id) = resolved.result.object.object_id {
                    let mut call = CallFunctionOnParams::new(GEOMETRY);
                    call.object_id = Some(object_id);
                    call.return_by_value = Some(true);
                    let called = page.execute(call).await.map_err(protocol)?;
                    if let Some(value) = called.result.result.value {
                        if !value.is_null() {
                            let geometry: Geometry =
                                serde_json::from_value(value).map_err(protocol)?;
                            raw.bounds = Some(Bounds {
                                x: geometry.x,
                                y: geometry.y,
                                width: geometry.width,
                                height: geometry.height,
                            });
                            raw.text = match (geometry.size, geometry.weight, geometry.color) {
                                (Some(size), Some(weight), Some(color)) => {
                                    srgb(&color).map(|color| TextStyle { size, weight, color })
                                }
                                _ => None,
                            };
                            raw.placeholder = geometry.placeholder;
                        }
                    }
                }
            }
        }
        nodes.push(raw);
    }
    build(&nodes)
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
