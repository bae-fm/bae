//! A design-system component's state as a board: the named row of the
//! component's `preview.html`.
//!
//! A preview page is a bare script that draws its rows with `React` and
//! `window.Bae`, so it is loaded the way the design system's README sets out:
//! the token sheet, the style sheet, React 18.3.1 (both files pinned by hash)
//! and the component script, then the page. Each row is a
//! `.bae-preview-row` whose `.bae-preview-name` holds the state's name and
//! whose `.bae-preview-cell` holds the drawing. A preview with no rows is one
//! state, `default`, drawn by the whole page.

use crate::design::Frame;
use crate::names::{state_id, Platform};
use crate::render::board::{self, BoardError};
use crate::render::read;
use crate::render::Size;
use crate::shots::Element;
use chromiumoxide::cdp::browser_protocol::dom::DescribeNodeParams;
use chromiumoxide::cdp::browser_protocol::page::{CaptureScreenshotFormat, Viewport};
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::Page;
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(crate) const REACT: &str = "bae-shots/canvas/react.production.min.js";
pub(crate) const REACT_DOM: &str = "bae-shots/canvas/react-dom.production.min.js";
const REACT_SHA256: &str = "d949f1c3687aedadcedac85261865f29b17cd273997e7f6b2bfc53b2f9d4c4dd";
const REACT_DOM_SHA256: &str = "35f4f974f4b2bcd44da73963347f8952e341f83909e4498227d4e26b98f66f0d";

/// The page the previews are loaded in, in points; a row draws at its own
/// size inside it.
const PREVIEW_FRAME: Frame = Frame { w: 1000, h: 800 };

const PREVIEW_READY: &str = r#"(() => {
  const root = document.querySelector('#root');
  if (!root || root.childElementCount === 0) return 'nothing drawn in #root';
  if (document.fonts.status !== 'loaded') return 'fonts ' + document.fonts.status;
  return 'ready';
})()"#;

#[derive(Debug, thiserror::Error)]
pub(crate) enum PreviewError {
    #[error(transparent)]
    Board(#[from] BoardError),
    #[error(transparent)]
    Read(#[from] read::ReadError),
    #[error("{path} is {actual}, not the pinned {expected}")]
    React { path: PathBuf, expected: &'static str, actual: String },
    #[error("cannot read {path}: {reason}")]
    Unreadable { path: PathBuf, reason: String },
    #[error("{component} has no row for the state '{state}'; its rows are {}", .rows.join(", "))]
    NoRow { component: String, state: String, rows: Vec<String> },
    #[error("{0} has no <head> to load the design system in")]
    NoHead(String),
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Checks that the copies of React are the pinned files.
pub(crate) fn check_react(root: &Path) -> Result<(), PreviewError> {
    for (file, expected) in [(REACT, REACT_SHA256), (REACT_DOM, REACT_DOM_SHA256)] {
        let path = root.join(file);
        let bytes = std::fs::read(&path)
            .map_err(|e| PreviewError::Unreadable { path: path.clone(), reason: e.to_string() })?;
        let actual = hex(&bytes);
        if actual != expected {
            return Err(PreviewError::React { path, expected, actual });
        }
    }
    Ok(())
}

/// The preview page with the design system loaded ahead of its script.
pub(crate) fn assemble(component: &str, html: &str) -> Result<String, PreviewError> {
    let head = Regex::new(r"(?i)<head[^>]*>").unwrap();
    let Some(found) = head.find(html) else {
        return Err(PreviewError::NoHead(component.to_string()));
    };
    let loads = r#"<link rel="stylesheet" href="/ds/bae/tokens.css"><link rel="stylesheet" href="/ds/bae/components/bundle.css"><script src="/react.js"></script><script src="/react-dom.js"></script><script src="/ds/bae/components/bundle.js"></script>"#;
    Ok(format!("{}{loads}{}", &html[..found.end()], &html[found.end()..]))
}

#[derive(Deserialize)]
struct Cell {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// The names of the page's rows.
const ROW_NAMES: &str = r#"[...document.querySelectorAll('.bae-preview-row')]
  .map(row => (row.querySelector('.bae-preview-name') || {}).textContent || '')"#;

/// A state's picture and element list.
pub(crate) struct Row {
    pub(crate) png: Vec<u8>,
    /// The cell's size in points.
    pub(crate) size: Size,
    pub(crate) elements: Vec<Element>,
}

/// Loads the preview at `url` and draws the row of `state`, at `scale` pixels
/// per point: the picture of its cell and the elements inside it, measured
/// from the cell's top-left, read as `platform`'s boards are.
pub(crate) async fn render_row(
    page: &Page,
    component: &str,
    state: &str,
    url: &str,
    scale: f64,
    platform: Platform,
) -> Result<Row, PreviewError> {
    board::load(page, component, url, PREVIEW_FRAME, scale, PREVIEW_READY).await?;
    let names: Vec<String> = page
        .evaluate(ROW_NAMES)
        .await
        .map_err(board::protocol)?
        .into_value()
        .map_err(board::protocol)?;
    let cell = if names.is_empty() && state == "default" {
        "document.body".to_string()
    } else {
        let index = names.iter().position(|name| state_id(name) == state).ok_or_else(|| {
            PreviewError::NoRow {
                component: component.to_string(),
                state: state.to_string(),
                rows: names.iter().map(|n| state_id(n)).collect(),
            }
        })?;
        format!("document.querySelectorAll('.bae-preview-row')[{index}].querySelector('.bae-preview-cell')")
    };
    let rect: Cell = page
        .evaluate(format!(
            "(() => {{ const r = {cell}.getBoundingClientRect(); \
             return {{x: r.x + scrollX, y: r.y + scrollY, width: r.width, height: r.height}}; }})()"
        ))
        .await
        .map_err(board::protocol)?
        .into_value()
        .map_err(board::protocol)?;
    let object = page
        .execute(EvaluateParams::builder().expression(cell).build().map_err(board::protocol)?)
        .await
        .map_err(board::protocol)?
        .result
        .result
        .object_id
        .ok_or_else(|| board::protocol("the row's cell is not an object"))?;
    let described = page
        .execute(DescribeNodeParams::builder().object_id(object).build())
        .await
        .map_err(board::protocol)?;
    let clip = Viewport::builder()
        .x(rect.x)
        .y(rect.y)
        .width(rect.width)
        .height(rect.height)
        .scale(1.0)
        .build()
        .map_err(board::protocol)?;
    let png = page
        .screenshot(
            ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Png)
                .clip(clip)
                .capture_beyond_viewport(true)
                .build(),
        )
        .await
        .map_err(board::protocol)?;
    let elements = read::elements_in(
        page,
        *described.result.node.backend_node_id.inner(),
        (rect.x, rect.y),
        platform,
    )
    .await?;
    Ok(Row { png, size: Size { width: rect.width, height: rect.height }, elements })
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
