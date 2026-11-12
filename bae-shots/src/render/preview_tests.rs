use super::*;
use crate::design;
use crate::render::fixture;
use crate::shots::Role;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn the_design_system_loads_ahead_of_the_previews_script() {
    let html = "<!-- card -->\n<!doctype html>\n<html>\n<head><meta charset=\"utf-8\"></head>\n<body><script>row()</script></body></html>";
    let page = assemble("MacRow", html).unwrap();
    let head = page.find("<head>").unwrap();
    let order: Vec<usize> =
        ["tokens.css", "bundle.css", "/react.js", "/react-dom.js", "bundle.js", "<meta", "row()"]
            .iter()
            .map(|needle| page.find(needle).unwrap_or_else(|| panic!("{needle} missing in {page}")))
            .collect();
    assert!(order[0] > head);
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
}

#[test]
fn a_preview_with_no_head_is_refused() {
    assert!(matches!(assemble("MacRow", "<body></body>"), Err(PreviewError::NoHead(_))));
}

#[test]
fn the_vendored_copies_of_react_are_the_pinned_files() {
    check_react(&repository()).unwrap();
}

#[test]
fn another_copy_of_react_is_refused_naming_both_hashes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("bae-shots/canvas")).unwrap();
    std::fs::write(root.path().join(REACT), "other").unwrap();
    std::fs::write(root.path().join(REACT_DOM), "other").unwrap();
    let message = check_react(root.path()).unwrap_err().to_string();
    assert!(message.contains(REACT_SHA256), "{message}");
    assert!(message.contains(&hex(b"other")), "{message}");
}

#[tokio::test]
async fn a_row_reads_as_the_state_it_names() {
    let root = repository();
    design::build(&root).unwrap();
    let (row, missing) = fixture::with_browser(&root, async |server, chrome| {
        let page = chrome.new_page().await.unwrap();
        let url = server.preview_url("MacRow");
        let row = render_row(&page, "MacRow", "nested-off", &url, 2.0).await;
        let page = chrome.new_page().await.unwrap();
        let missing = render_row(&page, "MacRow", "missing", &url, 1.0).await;
        (row, missing)
    })
    .await;
    let message = missing.err().expect("a refusal").to_string();
    assert!(message.contains("nested-off") && message.contains("'missing'"), "{message}");
    let row = row.unwrap();
    let found: Vec<(Role, Option<&str>, Option<&str>)> =
        row.elements.iter().map(|e| (e.role, e.label.as_deref(), e.value.as_deref())).collect();
    assert_eq!(
        found,
        [
            (Role::Text, Some("Pause between sides and discs"), None),
            (Role::Toggle, Some("Pause"), Some("off")),
            (Role::Text, Some("Resume automatically"), None),
            (Role::Button, Some("Never"), None),
        ]
    );
    let switch = &row.elements[1];
    assert!(switch.bounds.x > 700.0 && switch.bounds.y > 0.0, "{:?}", switch.bounds);
    assert!(row.elements.iter().all(|e| e.bounds.y < row.size.height), "boxes are inside the cell");
    assert_eq!(&row.png[..8], b"\x89PNG\r\n\x1a\n");
    let width = u32::from_be_bytes(row.png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(row.png[20..24].try_into().unwrap());
    assert_eq!(
        (f64::from(width), f64::from(height)),
        (row.size.width * 2.0, row.size.height * 2.0)
    );
}
