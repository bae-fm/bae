use super::match_list::MatchList;
use super::source_selector::SearchSourceSelector;
use crate::import::MatchCandidate;
use crate::musicbrainz::extract_search_tokens;
use crate::ui::import_context::state::SearchTab;
use crate::ui::import_context::ImportContext;
use dioxus::prelude::*;
use std::rc::Rc;

/// Which search field is currently focused (for pill insertion)
#[derive(Debug, Clone, Copy, PartialEq)]
enum FocusedField {
    Artist,
    Album,
    Year,
    Label,
    CatalogNumber,
    Barcode,
}

#[component]
pub fn ManualSearchPanel(
    detected_metadata: Signal<Option<crate::import::FolderMetadata>>,
    on_match_select: EventHandler<usize>,
    on_confirm: EventHandler<MatchCandidate>,
    selected_index: Signal<Option<usize>>,
) -> Element {
    let import_context = use_context::<Rc<ImportContext>>();

    // Search fields from context
    let mut search_artist = import_context.search_artist();
    let mut search_album = import_context.search_album();
    let mut search_year = import_context.search_year();
    let mut search_label = import_context.search_label();
    let mut search_catalog_number = import_context.search_catalog_number();
    let mut search_barcode = import_context.search_barcode();
    let mut active_tab = import_context.search_tab();

    let search_source = import_context.search_source();
    let match_candidates = import_context.manual_match_candidates();
    let mut is_searching = use_signal(|| false);
    let error_message = import_context.error_message();

    // Track which field is focused for pill insertion
    let mut focused_field = use_signal(|| None::<FocusedField>);

    // Element references for focusing after clear
    let mut artist_input_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut album_input_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut year_input_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut label_input_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let catno_input_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let barcode_input_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    // Extract search tokens from detected metadata
    let search_tokens = use_memo(move || {
        detected_metadata
            .read()
            .as_ref()
            .map(extract_search_tokens)
            .unwrap_or_default()
    });

    // Search handlers for each section
    let mut perform_general_search = {
        let import_context = import_context.clone();
        move || {
            let artist = search_artist.read().clone();
            let album = search_album.read().clone();
            let year = search_year.read().clone();
            let label = search_label.read().clone();

            if artist.trim().is_empty()
                && album.trim().is_empty()
                && year.trim().is_empty()
                && label.trim().is_empty()
            {
                import_context
                    .set_error_message(Some("Please fill in at least one field".to_string()));
                return;
            }

            is_searching.set(true);
            import_context.set_error_message(None);
            import_context.set_manual_match_candidates(Vec::new());

            let import_context_clone = import_context.clone();
            let source = search_source.read().clone();
            let mut is_searching_clone = is_searching;

            spawn(async move {
                match import_context_clone
                    .search_general(source, artist, album, year, label)
                    .await
                {
                    Ok(candidates) => {
                        import_context_clone.set_manual_match_candidates(candidates);
                    }
                    Err(e) => {
                        import_context_clone
                            .set_error_message(Some(format!("Search failed: {}", e)));
                    }
                }
                import_context_clone.set_has_searched(true);
                is_searching_clone.set(false);
            });
        }
    };

    let mut perform_catno_search = {
        let import_context = import_context.clone();
        move || {
            let catno = search_catalog_number.read().clone();

            if catno.trim().is_empty() {
                import_context.set_error_message(Some("Please enter a catalog number".to_string()));
                return;
            }

            is_searching.set(true);
            import_context.set_error_message(None);
            import_context.set_manual_match_candidates(Vec::new());

            let import_context_clone = import_context.clone();
            let source = search_source.read().clone();
            let mut is_searching_clone = is_searching;

            spawn(async move {
                match import_context_clone
                    .search_by_catalog_number(source, catno)
                    .await
                {
                    Ok(candidates) => {
                        import_context_clone.set_manual_match_candidates(candidates);
                    }
                    Err(e) => {
                        import_context_clone
                            .set_error_message(Some(format!("Search failed: {}", e)));
                    }
                }
                import_context_clone.set_has_searched(true);
                is_searching_clone.set(false);
            });
        }
    };

    let mut perform_barcode_search = {
        let import_context = import_context.clone();
        move || {
            let barcode = search_barcode.read().clone();

            if barcode.trim().is_empty() {
                import_context.set_error_message(Some("Please enter a barcode".to_string()));
                return;
            }

            is_searching.set(true);
            import_context.set_error_message(None);
            import_context.set_manual_match_candidates(Vec::new());

            let import_context_clone = import_context.clone();
            let source = search_source.read().clone();
            let mut is_searching_clone = is_searching;

            spawn(async move {
                match import_context_clone
                    .search_by_barcode(source, barcode)
                    .await
                {
                    Ok(candidates) => {
                        import_context_clone.set_manual_match_candidates(candidates);
                    }
                    Err(e) => {
                        import_context_clone
                            .set_error_message(Some(format!("Search failed: {}", e)));
                    }
                }
                import_context_clone.set_has_searched(true);
                is_searching_clone.set(false);
            });
        }
    };

    let has_searched = import_context.has_searched();

    rsx! {
        div { class: "bg-gray-800 rounded-lg shadow p-6 space-y-4",
            // Header with title and source selector
            div { class: "flex justify-between items-center",
                h3 { class: "text-lg font-semibold text-white", "Search for Release" }
                SearchSourceSelector {
                    selected_source: search_source,
                    on_select: move |source| {
                        import_context.set_search_source(source);
                        import_context.set_manual_match_candidates(Vec::new());
                        import_context.set_error_message(None);
                    },
                }
            }

            // Tabs
            div { class: "flex border-b border-gray-700",
                button {
                    class: if *active_tab.read() == SearchTab::General { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white" },
                    onclick: move |_| {
                        active_tab.set(SearchTab::General);
                    },
                    "General"
                }
                button {
                    class: if *active_tab.read() == SearchTab::CatalogNumber { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white" },
                    onclick: move |_| {
                        active_tab.set(SearchTab::CatalogNumber);
                    },
                    "Catalog #"
                }
                button {
                    class: if *active_tab.read() == SearchTab::Barcode { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white" },
                    onclick: move |_| {
                        active_tab.set(SearchTab::Barcode);
                    },
                    "Barcode"
                }
            }

            // Search tokens as clickable pills
            if !search_tokens.read().is_empty() {
                div { class: "flex flex-wrap gap-2",
                    for token in search_tokens.read().iter() {
                        {
                            let token_clone = token.clone();
                            let is_enabled = focused_field.read().is_some();
                            let pill_class = if is_enabled {
                                "px-3 py-1 text-sm bg-gray-700 text-gray-300 rounded-full hover:bg-gray-600 hover:text-white transition-colors border border-gray-600 cursor-pointer"
                            } else {
                                "px-3 py-1 text-sm bg-gray-800 text-gray-500 rounded-full border border-gray-700 cursor-not-allowed opacity-60"
                            };
                            rsx! {
                                button {
                                    class: "{pill_class}",
                                    disabled: !is_enabled,
                                    title: if is_enabled { "Click to fill focused field" } else { "Focus a field first, then click to fill" },
                                    onmousedown: move |e| {
                                        e.prevent_default();
                                        if let Some(field) = *focused_field.read() {
                                            match field {
                                                FocusedField::Artist => {
                                                    search_artist.set(token_clone.clone());
                                                    // Advance to next field in general search
                                                    if let Some(input) = album_input_ref.read().as_ref() {
                                                        std::mem::drop(input.set_focus(true));
                                                    }
                                                }
                                                FocusedField::Album => {
                                                    search_album.set(token_clone.clone());
                                                    // Advance to next field in general search
                                                    if let Some(input) = year_input_ref.read().as_ref() {
                                                        std::mem::drop(input.set_focus(true));
                                                    }
                                                }
                                                FocusedField::Year => {
                                                    search_year.set(token_clone.clone());
                                                    // Advance to next field in general search
                                                    if let Some(input) = label_input_ref.read().as_ref() {
                                                        std::mem::drop(input.set_focus(true));
                                                    }
                                                }
                                                FocusedField::Label => {
                                                    search_label.set(token_clone.clone());
                                                    // Stay on Label (last field)
                                                }
                                                FocusedField::CatalogNumber => {
                                                    search_catalog_number.set(token_clone.clone());
                                                    // Stay on CatalogNumber (single field form)
                                                }
                                                FocusedField::Barcode => {
                                                    search_barcode.set(token_clone.clone());
                                                    // Stay on Barcode (single field form)
                                                }
                                            }
                                        }
                                    },
                                    "{token}"
                                }
                            }
                        }
                    }
                }
            }

            // Tab content
            div { class: "space-y-3",
                match *active_tab.read() {
                    SearchTab::General => rsx! {
                        div { class: "grid grid-cols-2 gap-3",
                            // Artist field
                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-1", "Artist" }
                                div { class: "relative",
                                    input {
                                        r#type: "text",
                                        class: "w-full px-4 py-2 pr-10 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                        value: "{search_artist}",
                                        oninput: move |e| search_artist.set(e.value()),
                                        onfocus: move |_| focused_field.set(Some(FocusedField::Artist)),
                                        onblur: move |_| focused_field.set(None),
                                        onmounted: move |element| {
                                            let data = element.data();
                                            artist_input_ref.set(Some(data.clone()));
                                        },
                                    }
                                    if !search_artist.read().is_empty() {
                                        button {
                                            class: "absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white p-1",
                                            onclick: move |_| {
                                                search_artist.set(String::new());
                                                if let Some(input) = artist_input_ref.read().as_ref() {
                                                    drop(input.set_focus(true));
                                                }
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }

                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-1", "Album" }
                                div { class: "relative",
                                    input {
                                        r#type: "text",
                                        class: "w-full px-4 py-2 pr-10 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                        value: "{search_album}",
                                        oninput: move |e| search_album.set(e.value()),
                                        onfocus: move |_| focused_field.set(Some(FocusedField::Album)),
                                        onblur: move |_| focused_field.set(None),
                                        onmounted: move |element| album_input_ref.set(Some(element.data())),
                                    }
                                    if !search_album.read().is_empty() {
                                        button {
                                            class: "absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white p-1",
                                            onclick: move |_| {
                                                search_album.set(String::new());
                                                if let Some(input) = album_input_ref.read().as_ref() {
                                                    std::mem::drop(input.set_focus(true));
                                                }
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-1", "Year" }
                                div { class: "relative",
                                    input {
                                        r#type: "text",
                                        class: "w-full px-4 py-2 pr-10 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                        value: "{search_year}",
                                        oninput: move |e| search_year.set(e.value()),
                                        onfocus: move |_| focused_field.set(Some(FocusedField::Year)),
                                        onblur: move |_| focused_field.set(None),
                                        onmounted: move |element| year_input_ref.set(Some(element.data())),
                                    }
                                    if !search_year.read().is_empty() {
                                        button {
                                            class: "absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white p-1",
                                            onclick: move |_| {
                                                search_year.set(String::new());
                                                if let Some(input) = year_input_ref.read().as_ref() {
                                                    std::mem::drop(input.set_focus(true));
                                                }
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-1", "Label" }
                                div { class: "relative",
                                    input {
                                        r#type: "text",
                                        class: "w-full px-4 py-2 pr-10 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                        value: "{search_label}",
                                        oninput: move |e| search_label.set(e.value()),
                                        onfocus: move |_| focused_field.set(Some(FocusedField::Label)),
                                        onblur: move |_| focused_field.set(None),
                                        onmounted: move |element| label_input_ref.set(Some(element.data())),
                                    }
                                    if !search_label.read().is_empty() {
                                        button {
                                            class: "absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white p-1",
                                            onclick: move |_| {
                                                search_label.set(String::new());
                                                if let Some(input) = label_input_ref.read().as_ref() {
                                                    std::mem::drop(input.set_focus(true));
                                                }
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "flex justify-end pt-2",
                            button {
                                class: "px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_searching.read(),
                                onclick: move |_| perform_general_search(),
                                if *is_searching.read() {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                    SearchTab::CatalogNumber => rsx! {
                        div { class: "flex gap-3",
                            div { class: "flex-1 relative",
                                input {
                                    key: "catno-input",
                                    r#type: "text",
                                    class: "w-full px-4 py-2 pr-10 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                    value: "{search_catalog_number}",
                                    oninput: move |e| search_catalog_number.set(e.value()),
                                    onfocus: move |_| focused_field.set(Some(FocusedField::CatalogNumber)),
                                    onblur: move |_| focused_field.set(None),
                                    onmounted: |element| {
                                        println!("catno-input mounted");
                                        drop(element.set_focus(true));
                                    },
                                }
                                if !search_catalog_number.read().is_empty() {
                                    button {
                                        class: "absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white p-1",
                                        onclick: move |_| {
                                            search_catalog_number.set(String::new());
                                            if let Some(input) = catno_input_ref.read().as_ref() {
                                                std::mem::drop(input.set_focus(true));
                                            }
                                        },
                                        "✕"
                                    }
                                }
                            }
                            button {
                                class: "px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_searching.read(),
                                onclick: move |_| perform_catno_search(),
                                if *is_searching.read() {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                    SearchTab::Barcode => rsx! {
                        div { class: "flex gap-3",
                            div { class: "flex-1 relative",
                                input {
                                    key: "barcode-input",
                                    r#type: "text",
                                    class: "w-full px-4 py-2 pr-10 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                    value: "{search_barcode}",
                                    oninput: move |e| search_barcode.set(e.value()),
                                    onfocus: move |_| focused_field.set(Some(FocusedField::Barcode)),
                                    onblur: move |_| focused_field.set(None),
                                    onmounted: |element| {
                                        drop(element.set_focus(true));
                                    },
                                }
                                if !search_barcode.read().is_empty() {
                                    button {
                                        class: "absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white p-1",
                                        onclick: move |_| {
                                            search_barcode.set(String::new());
                                            if let Some(input) = barcode_input_ref.read().as_ref() {
                                                std::mem::drop(input.set_focus(true));
                                            }
                                        },
                                        "✕"
                                    }
                                }
                            }
                            button {
                                class: "px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_searching.read(),
                                onclick: move |_| perform_barcode_search(),
                                if *is_searching.read() {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                }
            }

            // Error display
            if let Some(ref error) = error_message.read().as_ref() {
                div { class: "bg-red-900/30 border border-red-700 rounded-lg p-4",
                    p { class: "text-sm text-red-300 select-text", "Error: {error}" }
                }
            }

            // Results display
            if *is_searching.read() {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "Searching..." }
                }
            } else if match_candidates.read().is_empty() && *has_searched.read() {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "No results found" }
                }
            } else if !match_candidates.read().is_empty() {
                div { class: "space-y-4 mt-4",
                    MatchList {
                        candidates: match_candidates.read().clone(),
                        selected_index: selected_index.read().as_ref().copied(),
                        on_select: move |index| {
                            selected_index.set(Some(index));
                            on_match_select.call(index);
                        },
                    }
                    if selected_index.read().is_some() {
                        div { class: "flex justify-end",
                            button {
                                class: "px-6 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700",
                                onclick: move |_| {
                                    if let Some(index) = selected_index.read().as_ref().copied() {
                                        if let Some(candidate) = match_candidates.read().get(index) {
                                            on_confirm.call(candidate.clone());
                                        }
                                    }
                                },
                                "Confirm Selection"
                            }
                        }
                    }
                }
            }
        }
    }
}
