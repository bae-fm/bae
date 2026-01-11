//! Manual search panel wrapper - reads context and delegates to ManualSearchPanelView

use crate::import::MatchCandidate;
use crate::musicbrainz::extract_search_tokens;
use crate::ui::components::import::workflow::shared::confirmation::to_display_candidate;
use crate::ui::import_context::ImportContext;
use bae_ui::components::import::ManualSearchPanelView;
use bae_ui::display_types::{SearchSource, SearchTab};
use dioxus::prelude::*;
use std::rc::Rc;

use crate::ui::components::import::SearchSource as BaeSearchSource;
use crate::ui::import_context::SearchTab as BaeSearchTab;

/// Convert bae SearchSource to bae-ui display type
fn to_display_search_source(source: &BaeSearchSource) -> SearchSource {
    match source {
        BaeSearchSource::MusicBrainz => SearchSource::MusicBrainz,
        BaeSearchSource::Discogs => SearchSource::Discogs,
    }
}

/// Convert bae SearchTab to bae-ui display type
fn to_display_search_tab(tab: &BaeSearchTab) -> SearchTab {
    match tab {
        BaeSearchTab::General => SearchTab::General,
        BaeSearchTab::CatalogNumber => SearchTab::CatalogNumber,
        BaeSearchTab::Barcode => SearchTab::Barcode,
    }
}

#[component]
pub fn ManualSearchPanel(
    detected_metadata: Signal<Option<crate::import::FolderMetadata>>,
    on_match_select: EventHandler<usize>,
    on_confirm: EventHandler<MatchCandidate>,
    selected_index: Signal<Option<usize>>,
) -> Element {
    let import_context = use_context::<Rc<ImportContext>>();
    let search_artist = import_context.search_artist();
    let search_album = import_context.search_album();
    let search_year = import_context.search_year();
    let search_label = import_context.search_label();
    let search_catalog_number = import_context.search_catalog_number();
    let search_barcode = import_context.search_barcode();
    let active_tab = import_context.search_tab();
    let search_source = import_context.search_source();
    let match_candidates = import_context.manual_match_candidates();
    let mut is_searching = use_signal(|| false);
    let error_message = import_context.error_message();
    let has_searched = import_context.has_searched();

    let search_tokens: Vec<String> = detected_metadata
        .read()
        .as_ref()
        .map(extract_search_tokens)
        .unwrap_or_default();

    // Search handlers
    let mut perform_search = {
        let import_context = import_context.clone();
        move || {
            let tab = *active_tab.read();
            let source = search_source.read().clone();

            match tab {
                BaeSearchTab::General => {
                    let artist = search_artist.read().clone();
                    let album = search_album.read().clone();
                    let year = search_year.read().clone();
                    let label = search_label.read().clone();

                    if artist.trim().is_empty()
                        && album.trim().is_empty()
                        && year.trim().is_empty()
                        && label.trim().is_empty()
                    {
                        import_context.set_error_message(Some(
                            "Please fill in at least one field".to_string(),
                        ));
                        return;
                    }

                    is_searching.set(true);
                    import_context.set_error_message(None);
                    import_context.set_manual_match_candidates(Vec::new());

                    let ctx = import_context.clone();
                    let mut is_searching = is_searching;
                    spawn(async move {
                        match ctx.search_general(source, artist, album, year, label).await {
                            Ok(candidates) => ctx.set_manual_match_candidates(candidates),
                            Err(e) => ctx.set_error_message(Some(format!("Search failed: {}", e))),
                        }
                        ctx.set_has_searched(true);
                        is_searching.set(false);
                    });
                }
                BaeSearchTab::CatalogNumber => {
                    let catno = search_catalog_number.read().clone();
                    if catno.trim().is_empty() {
                        import_context
                            .set_error_message(Some("Please enter a catalog number".to_string()));
                        return;
                    }

                    is_searching.set(true);
                    import_context.set_error_message(None);
                    import_context.set_manual_match_candidates(Vec::new());

                    let ctx = import_context.clone();
                    let mut is_searching = is_searching;
                    spawn(async move {
                        match ctx.search_by_catalog_number(source, catno).await {
                            Ok(candidates) => ctx.set_manual_match_candidates(candidates),
                            Err(e) => ctx.set_error_message(Some(format!("Search failed: {}", e))),
                        }
                        ctx.set_has_searched(true);
                        is_searching.set(false);
                    });
                }
                BaeSearchTab::Barcode => {
                    let barcode = search_barcode.read().clone();
                    if barcode.trim().is_empty() {
                        import_context
                            .set_error_message(Some("Please enter a barcode".to_string()));
                        return;
                    }

                    is_searching.set(true);
                    import_context.set_error_message(None);
                    import_context.set_manual_match_candidates(Vec::new());

                    let ctx = import_context.clone();
                    let mut is_searching = is_searching;
                    spawn(async move {
                        match ctx.search_by_barcode(source, barcode).await {
                            Ok(candidates) => ctx.set_manual_match_candidates(candidates),
                            Err(e) => ctx.set_error_message(Some(format!("Search failed: {}", e))),
                        }
                        ctx.set_has_searched(true);
                        is_searching.set(false);
                    });
                }
            }
        }
    };

    // Convert candidates to display type
    let display_candidates: Vec<bae_ui::display_types::MatchCandidate> = match_candidates
        .read()
        .iter()
        .map(to_display_candidate)
        .collect();

    rsx! {
        ManualSearchPanelView {
            search_source: to_display_search_source(&search_source.read()),
            on_search_source_change: {
                let import_context = import_context.clone();
                move |source: SearchSource| {
                    let bae_source = match source {
                        SearchSource::MusicBrainz => BaeSearchSource::MusicBrainz,
                        SearchSource::Discogs => BaeSearchSource::Discogs,
                    };
                    import_context.set_search_source(bae_source);
                    import_context.set_manual_match_candidates(Vec::new());
                    import_context.set_error_message(None);
                }
            },
            active_tab: to_display_search_tab(&active_tab.read()),
            on_tab_change: {
                let import_context = import_context.clone();
                move |tab: SearchTab| {
                    let bae_tab = match tab {
                        SearchTab::General => BaeSearchTab::General,
                        SearchTab::CatalogNumber => BaeSearchTab::CatalogNumber,
                        SearchTab::Barcode => BaeSearchTab::Barcode,
                    };
                    import_context.set_search_tab(bae_tab);
                }
            },
            search_artist: search_artist.read().clone(),
            on_artist_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_artist(value)
            },
            search_album: search_album.read().clone(),
            on_album_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_album(value)
            },
            search_year: search_year.read().clone(),
            on_year_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_year(value)
            },
            search_label: search_label.read().clone(),
            on_label_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_label(value)
            },
            search_catalog_number: search_catalog_number.read().clone(),
            on_catalog_number_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_catalog_number(value)
            },
            search_barcode: search_barcode.read().clone(),
            on_barcode_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_barcode(value)
            },
            search_tokens,
            is_searching: *is_searching.read(),
            error_message: error_message.read().clone(),
            has_searched: *has_searched.read(),
            match_candidates: display_candidates,
            selected_index: *selected_index.read(),
            on_match_select: move |index: usize| {
                selected_index.set(Some(index));
                on_match_select.call(index);
            },
            on_search: move |_| perform_search(),
            on_confirm: move |candidate: bae_ui::display_types::MatchCandidate| {
                // Find the original bae candidate by title match
                if let Some(bae_candidate) = match_candidates
                    .read()
                    .iter()
                    .find(|c| c.title() == candidate.title)
                {
                    on_confirm.call(bae_candidate.clone());
                }
            },
        }
    }
}
