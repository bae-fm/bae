//! Rendering the design boards.

pub(crate) mod board;
pub(crate) mod chrome;
#[cfg(test)]
mod fixture;
pub(crate) mod preview;
pub(crate) mod read;
pub(crate) mod runtime;
pub(crate) mod serve;

use crate::design;
use crate::names::{self, Name};
use std::path::Path;

#[derive(serde::Serialize)]
pub(crate) struct Size {
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// The element list a board writes beside its picture, in the shape the shots
/// write theirs.
#[derive(serde::Serialize)]
struct BoardList<'a> {
    board: &'a str,
    platform: &'a str,
    size: Size,
    scale: f64,
    elements: Vec<crate::shots::Element>,
}

/// Pixels per point the boards are drawn at.
const BOARD_SCALE: f64 = 1.0;

/// Renders the board of each screen shot in `names` into `out` as
/// `<name>.png`, after the design builds and the runtime checks.
pub(crate) fn boards(root: &Path, names: &[Name], out: &Path) -> Result<(), String> {
    let runtime = runtime::check(root).map_err(|e| e.to_string())?;
    design::build(root).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| format!("cannot prepare {}: {e}", out.display()))?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(draw(root, names, out, &runtime))
}

async fn draw(
    root: &Path,
    names: &[Name],
    out: &Path,
    runtime: &runtime::Checked,
) -> Result<(), String> {
    serve::with_server(root, async |server| {
        chrome::with_chrome(async |chrome| {
            draw_with(root, names, out, runtime, server, chrome).await
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One board drawn: its picture, size and elements.
struct Drawn {
    hash: String,
    png: Vec<u8>,
    size: Size,
    elements: Vec<crate::shots::Element>,
}

async fn draw_with(
    root: &Path,
    names: &[Name],
    out: &Path,
    runtime: &runtime::Checked,
    server: &serve::Server,
    chrome: &chrome::Chrome,
) -> Result<(), String> {
    for name in names {
        let board = name.to_string();
        let page = chrome.new_page().await.map_err(|e| e.to_string())?;
        let drawn = match name {
            Name::Screen(_) => draw_screen(root, &board, &page, runtime, server, chrome).await,
            Name::Component(component) => {
                draw_component(root, component, &page, server, chrome).await
            }
        };
        if let Err(error) = page.close().await {
            eprintln!("bae-shots: closing a page failed: {error}");
        }
        let drawn = drawn?;
        let file = out.join(format!("{board}.png"));
        std::fs::write(&file, &drawn.png)
            .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
        let list = BoardList {
            board: &board,
            platform: name.platform().file_word(),
            size: drawn.size,
            scale: BOARD_SCALE,
            elements: drawn.elements,
        };
        let file = out.join(format!("{board}.json"));
        std::fs::write(&file, serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?)
            .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
        eprintln!("bae-shots: rendered {board} (board {})", drawn.hash);
    }
    Ok(())
}

async fn draw_screen(
    root: &Path,
    board: &str,
    page: &chromiumoxide::Page,
    runtime: &runtime::Checked,
    server: &serve::Server,
    chrome: &chrome::Chrome,
) -> Result<Drawn, String> {
    let frame = design::frame(root, board).map_err(|e| e.to_string())?;
    let hash = design::board_hash(root, board, &runtime.sha256, chrome.version())
        .map_err(|e| e.to_string())?;
    let rendered = board::render(page, board, &server.board_url(board), frame, BOARD_SCALE)
        .await
        .map_err(|e| e.to_string())?;
    let elements = read::elements(page).await.map_err(|e| e.to_string())?;
    Ok(Drawn {
        hash,
        png: rendered.png,
        size: Size { width: f64::from(frame.w), height: f64::from(frame.h) },
        elements,
    })
}

async fn draw_component(
    root: &Path,
    name: &names::ComponentName,
    page: &chromiumoxide::Page,
    server: &serve::Server,
    chrome: &chrome::Chrome,
) -> Result<Drawn, String> {
    let component = name.component();
    preview::check_react(root).map_err(|e| e.to_string())?;
    let hash =
        design::preview_hash(root, &component, chrome.version()).map_err(|e| e.to_string())?;
    let row = preview::render_row(
        page,
        &component,
        name.state(),
        &server.preview_url(&component),
        BOARD_SCALE,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(Drawn { hash, png: row.png, size: row.size, elements: row.elements })
}
