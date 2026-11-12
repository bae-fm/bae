use super::*;
use crate::design::Frame;
use crate::render::board::render;
use crate::render::fixture;

const BOX: Bounds = Bounds { x: 0.0, y: 0.0, width: 10.0, height: 10.0 };

fn node(id: &str, role: &str, children: &[&str]) -> Raw {
    Raw {
        id: id.to_string(),
        chrome_role: role.to_string(),
        children: children.iter().map(|c| c.to_string()).collect(),
        bounds: Some(BOX),
        ..Raw::default()
    }
}

fn named(mut node: Raw, name: &str) -> Raw {
    node.name = Some(name.to_string());
    node
}

#[test]
fn chrome_roles_become_the_shots_words() {
    let words: Vec<Role> = [
        "button",
        "switch",
        "checkbox",
        "radio",
        "textbox",
        "searchbox",
        "combobox",
        "listbox",
        "tab",
        "link",
        "img",
        "slider",
        "progressbar",
        "StaticText",
        "heading",
    ]
    .iter()
    .map(|r| Role::of(r))
    .collect();
    use Role::*;
    assert_eq!(
        words,
        [
            Button, Toggle, Toggle, Radio, Field, Field, Popup, Popup, Tab, Link, Image, Slider,
            Progress, Text, Other
        ]
    );
}

#[test]
fn words_are_composed_collapsed_and_trimmed_and_nothing_else() {
    assert_eq!(words("  Cafe\u{301}  \n au   lait "), "Caf\u{e9} au lait");
    assert_eq!(words("\u{201c}Set up\u{201d}\u{2026}"), "\u{201c}Set up\u{201d}\u{2026}");
}

#[test]
fn a_controls_text_is_its_name_and_not_an_element() {
    let raw = [
        node("1", "RootWebArea", &["2"]),
        named(node("2", "button", &["3"]), "Add library\u{2026}"),
        named(node("3", "StaticText", &[]), "Add library\u{2026}"),
    ];
    let elements = build(&raw).unwrap();
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].role, Role::Button);
    assert_eq!(elements[0].label.as_deref(), Some("Add library\u{2026}"));
}

#[test]
fn ignored_nodes_are_left_out_and_their_children_attach_to_the_next_kept_ancestor() {
    let mut hidden = named(node("3", "StaticText", &[]), "Note");
    hidden.ignored = true;
    let mut wrapper = node("2", "group", &["3", "4"]);
    wrapper.ignored = true;
    let raw = [
        node("1", "list", &["2"]),
        wrapper,
        hidden,
        named(node("4", "StaticText", &[]), "Visible"),
    ];
    let elements = build(&raw).unwrap();
    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].native, "list");
    assert_eq!(elements[1].label.as_deref(), Some("Visible"));
    assert_eq!(elements[1].parent, Some(0));
}

#[test]
fn an_unnamed_container_is_structure_without_a_label() {
    let raw = [
        named(node("1", "group", &["2"]), "ignored name"),
        named(node("2", "StaticText", &[]), "Words"),
    ];
    let elements = build(&raw).unwrap();
    assert_eq!(elements[0].role, Role::Other);
    assert_eq!(elements[0].label, None);
    assert_eq!(elements[1].parent, Some(0));
}

#[test]
fn a_child_the_tree_does_not_hold_is_an_error() {
    let raw = [node("1", "list", &["2"])];
    assert!(matches!(build(&raw), Err(ReadError::NoNode(id)) if id == "2"));
}

#[test]
fn colours_are_srgb_with_alpha_or_absent() {
    assert_eq!(srgb("rgb(27, 27, 34)").as_deref(), Some("#1B1B22FF"));
    assert_eq!(srgb("rgba(255, 0, 128, 0.5)").as_deref(), Some("#FF008080"));
    assert_eq!(srgb("color(srgb 1 0 0)"), None);
}

const BOARD: &str = r##"<x-dc><helmet><style>body{margin:0;font:13px sans-serif}</style></helmet>
<div style="width:400px"><span id="short" style="font-weight:600">Short</span></div>
<button>Add <b>library</b>&hellip;</button>
<div role="switch" aria-checked="true" aria-label="Sync" tabindex="0">x</div>
<div role="switch" aria-checked="false" aria-label="Offline" tabindex="0">x</div>
<div role="radio" aria-checked="true" aria-label="Dark" tabindex="0">x</div>
<input type="text" aria-label="Name" placeholder="Library name" value="Odile">
<select aria-label="Mode"><option>Never</option><option selected>Always</option></select>
<div role="tab" aria-selected="true" tabindex="0">Library</div>
<a href="#x">More</a>
<img alt="Cover" width="10" height="10" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=">
<div role="slider" aria-valuenow="5" aria-valuemin="0" aria-valuemax="10" aria-label="Volume" tabindex="0">x</div>
<div role="progressbar" aria-valuenow="40" aria-valuemin="0" aria-valuemax="100" aria-label="Import">x</div>
<p aria-hidden="true">Note for the builder</p>
<p>Words   with
   <span>spans</span> mixed</p>
<button disabled>Off</button>
</x-dc>"##;

async fn read_fixture() -> Vec<Element> {
    let root = fixture::tree(&[("fixture.roles", BOARD)]);
    let elements = fixture::with_browser(root.path(), async |server, chrome| {
        let page = chrome.new_page().await.unwrap();
        let url = server.board_url("fixture.roles");
        render(&page, "fixture.roles", &url, Frame { w: 500, h: 600 }, 1.0).await.unwrap();
        elements(&page).await
    })
    .await;
    elements.unwrap()
}

fn find<'a>(elements: &'a [Element], role: Role, label: &str) -> &'a Element {
    elements
        .iter()
        .find(|e| e.role == role && e.label.as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no {role:?} {label:?} in {elements:#?}"))
}

#[tokio::test]
async fn a_board_reads_as_the_shots_do() {
    let elements = read_fixture().await;

    let button = find(&elements, Role::Button, "Add library\u{2026}");
    assert!(button.enabled);
    assert!(
        !elements
            .iter()
            .any(|e| e.role == Role::Text
                && e.label.as_deref().is_some_and(|l| l.starts_with("Add"))),
        "the button's text is its name only"
    );
    assert!(!find(&elements, Role::Button, "Off").enabled);

    assert_eq!(find(&elements, Role::Toggle, "Sync").value.as_deref(), Some("on"));
    assert_eq!(find(&elements, Role::Toggle, "Offline").value.as_deref(), Some("off"));
    assert_eq!(find(&elements, Role::Radio, "Dark").selected, Some(true));

    let field = find(&elements, Role::Field, "Name");
    assert_eq!(field.value.as_deref(), Some("Odile"));
    assert_eq!(field.placeholder.as_deref(), Some("Library name"));

    assert_eq!(find(&elements, Role::Popup, "Mode").value.as_deref(), Some("Always"));
    assert_eq!(find(&elements, Role::Tab, "Library").selected, Some(true));
    find(&elements, Role::Link, "More");
    find(&elements, Role::Image, "Cover");
    assert_eq!(find(&elements, Role::Slider, "Volume").value.as_deref(), Some("5"));
    assert_eq!(find(&elements, Role::Progress, "Import").value.as_deref(), Some("40"));

    assert!(
        !elements.iter().any(|e| e.label.as_deref().is_some_and(|l| l.contains("builder"))),
        "an aria-hidden note is left out"
    );
    find(&elements, Role::Text, "Words with");
    find(&elements, Role::Text, "spans");
    find(&elements, Role::Text, "mixed");
}

#[tokio::test]
async fn a_texts_box_is_its_words_not_its_container() {
    let elements = read_fixture().await;
    let short = find(&elements, Role::Text, "Short");
    assert!(short.bounds.width < 80.0, "{:?}", short.bounds);
    assert!(short.bounds.height > 10.0 && short.bounds.height < 30.0, "{:?}", short.bounds);
    let style = short.text.as_ref().unwrap();
    assert_eq!(style.size, 13.0);
    assert_eq!(style.weight, 600);
    assert_eq!(style.color.len(), 9);
}
