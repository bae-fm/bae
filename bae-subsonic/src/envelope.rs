//! The `<subsonic-response>` envelope and the one element model both encodings
//! emit from.
//!
//! Every response — success or error — wraps a payload in a `<subsonic-response>`
//! carrying `status`, `version`, `type`, `serverVersion`, and `openSubsonic`.
//! The `f` request parameter picks the wire form: `xml` (the default), `json`,
//! or `jsonp` (json wrapped in a `callback(...)` call).
//!
//! A response object is one [`Element`]: scalar fields are attributes, repeated
//! sub-objects are child elements. XML renders scalars as attributes and
//! children as nested tags; JSON renders scalars as object fields and groups
//! same-named children into arrays (the Subsonic JSON convention). Modeling the
//! object once and deriving both encodings keeps them from drifting.

use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::{Map, Value};

use crate::error::SubError;

/// The advertised Subsonic API version. 1.16.1 is the last documented version;
/// `openSubsonic="true"` signals the OpenSubsonic extensions on top of it.
const API_VERSION: &str = "1.16.1";
/// The `type` a Subsonic client reads to identify the server implementation.
const SERVER_TYPE: &str = "bae";

/// The build identifier reported as `serverVersion`.
fn server_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The serialization a request asked for. `xml` is the default when `f` is
/// absent or unrecognized.
#[derive(Debug, Clone)]
pub(crate) enum Format {
    Xml,
    Json,
    /// JSON wrapped in `callback(...)`; carries the callback name.
    Jsonp(String),
}

impl Format {
    /// Resolve the format from the `f` and `callback` query parameters. An
    /// unknown `f` falls back to XML, matching Subsonic clients' expectation
    /// that XML is the default. `jsonp` with no `callback` degrades to plain
    /// JSON — there is no function name to wrap it in.
    pub(crate) fn from_params(f: Option<&str>, callback: Option<&str>) -> Format {
        match f {
            Some("json") => Format::Json,
            Some("jsonp") => match callback {
                Some(name) if !name.is_empty() => Format::Jsonp(name.to_string()),
                _ => Format::Json,
            },
            _ => Format::Xml,
        }
    }
}

/// A scalar field value, carrying its type so JSON renders numbers and booleans
/// as JSON numbers/booleans while XML renders every attribute as text.
#[derive(Debug, Clone)]
pub(crate) enum Val {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl From<String> for Val {
    fn from(v: String) -> Self {
        Val::Str(v)
    }
}
impl From<&str> for Val {
    fn from(v: &str) -> Self {
        Val::Str(v.to_string())
    }
}
impl From<i64> for Val {
    fn from(v: i64) -> Self {
        Val::Int(v)
    }
}
impl From<bool> for Val {
    fn from(v: bool) -> Self {
        Val::Bool(v)
    }
}

/// One response object: a named node with scalar attributes and child objects.
#[derive(Debug, Clone)]
pub(crate) struct Element {
    pub(crate) name: &'static str,
    pub(crate) attrs: Vec<(&'static str, Val)>,
    pub(crate) children: Vec<Element>,
}

impl Element {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    pub(crate) fn attr(mut self, name: &'static str, value: impl Into<Val>) -> Self {
        self.attrs.push((name, value.into()));
        self
    }

    /// Add an attribute only when present. An absent optional field emits no
    /// attribute at all — distinct from an empty string.
    pub(crate) fn opt_attr(self, name: &'static str, value: Option<impl Into<Val>>) -> Self {
        match value {
            Some(v) => self.attr(name, v),
            None => self,
        }
    }

    pub(crate) fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    pub(crate) fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }

    /// Append this element's XML into `out`: `<name attr="v" …>` with child
    /// elements nested inside, or a self-closing tag when it has no children.
    fn write_xml(&self, out: &mut String) {
        out.push('<');
        out.push_str(self.name);
        for (name, value) in &self.attrs {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            out.push_str(&xml_escape_attr(&val_to_text(value)));
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for child in &self.children {
            child.write_xml(out);
        }
        out.push_str("</");
        out.push_str(self.name);
        out.push('>');
    }

    /// This element as a JSON object: attributes become fields, and children
    /// grouped by name become arrays (the Subsonic JSON convention — a repeated
    /// element is always an array, even with one member).
    fn to_json_object(&self) -> Value {
        let mut map = Map::new();
        for (name, value) in &self.attrs {
            map.insert((*name).to_string(), val_to_json(value));
        }
        // Preserve child order while grouping same-named children into arrays.
        let mut order: Vec<&'static str> = Vec::new();
        for child in &self.children {
            if !order.contains(&child.name) {
                order.push(child.name);
            }
        }
        for name in order {
            let group: Vec<Value> = self
                .children
                .iter()
                .filter(|c| c.name == name)
                .map(|c| c.to_json_object())
                .collect();
            map.insert(name.to_string(), Value::Array(group));
        }
        Value::Object(map)
    }
}

fn val_to_text(value: &Val) -> String {
    match value {
        Val::Str(s) => s.clone(),
        Val::Int(i) => i.to_string(),
        Val::Bool(b) => b.to_string(),
    }
}

fn val_to_json(value: &Val) -> Value {
    match value {
        Val::Str(s) => Value::String(s.clone()),
        Val::Int(i) => Value::Number((*i).into()),
        Val::Bool(b) => Value::Bool(*b),
    }
}

fn xml_escape_attr(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Render an OK response carrying an optional payload element (ping has none).
pub(crate) fn ok_response(format: &Format, payload: Option<Element>) -> Response {
    render(format, StatusCode::OK, "ok", payload, None)
}

/// Render a failed response carrying the error code and message.
pub(crate) fn error_response(format: &Format, error: &SubError) -> Response {
    render(format, StatusCode::OK, "failed", None, Some(error))
}

fn render(
    format: &Format,
    status: StatusCode,
    response_status: &str,
    payload: Option<Element>,
    error: Option<&SubError>,
) -> Response {
    let (content_type, body) = match format {
        Format::Xml => (
            "application/xml",
            render_xml(response_status, payload, error),
        ),
        Format::Json => (
            "application/json",
            render_json(response_status, payload, error).to_string(),
        ),
        Format::Jsonp(callback) => {
            let json = render_json(response_status, payload, error);
            ("application/javascript", format!("{callback}({json});"))
        }
    };
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .expect("subsonic envelope response is always valid")
}

fn render_xml(response_status: &str, payload: Option<Element>, error: Option<&SubError>) -> String {
    let mut out = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push_str(&format!(
        r#"<subsonic-response xmlns="http://subsonic.org/restapi" status="{status}" version="{version}" type="{server_type}" serverVersion="{server_version}" openSubsonic="true">"#,
        status = response_status,
        version = API_VERSION,
        server_type = SERVER_TYPE,
        server_version = server_version(),
    ));
    if let Some(error) = error {
        Element::new("error")
            .attr("code", error.code as i64)
            .attr("message", error.message.clone())
            .write_xml(&mut out);
    }
    if let Some(payload) = payload {
        payload.write_xml(&mut out);
    }
    out.push_str("</subsonic-response>");
    out
}

fn render_json(response_status: &str, payload: Option<Element>, error: Option<&SubError>) -> Value {
    let mut inner = Map::new();
    inner.insert(
        "status".to_string(),
        Value::String(response_status.to_string()),
    );
    inner.insert(
        "version".to_string(),
        Value::String(API_VERSION.to_string()),
    );
    inner.insert("type".to_string(), Value::String(SERVER_TYPE.to_string()));
    inner.insert(
        "serverVersion".to_string(),
        Value::String(server_version().to_string()),
    );
    inner.insert("openSubsonic".to_string(), Value::Bool(true));
    if let Some(error) = error {
        let mut e = Map::new();
        e.insert(
            "code".to_string(),
            Value::Number((error.code as i64).into()),
        );
        e.insert("message".to_string(), Value::String(error.message.clone()));
        inner.insert("error".to_string(), Value::Object(e));
    }
    if let Some(payload) = payload {
        // The single top-level payload is one object keyed by its element name;
        // only its nested repeated children become arrays.
        inner.insert(payload.name.to_string(), payload.to_json_object());
    }
    let mut root = Map::new();
    root.insert("subsonic-response".to_string(), Value::Object(inner));
    Value::Object(root)
}
