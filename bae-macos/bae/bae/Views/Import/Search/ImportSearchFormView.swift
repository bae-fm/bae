import BaeKit
import SwiftUI

/// The typed-search form, docked under the result area on every state: which
/// kind of query (General / Catalog # / Barcode), its fields with autocomplete
/// seeded from the folder's scanned text, and Search.
///
/// Every configured provider answers, so the form offers no source selection —
/// a provider that was never asked says so on its own line in the run above.
struct ImportSearchFormView: View {
    @Binding
    var activeTab: SearchTab
    @Binding
    var searchArtist: String
    @Binding
    var searchAlbum: String
    @Binding
    var searchCatalog: String
    @Binding
    var searchBarcode: String
    let signals: Signals?
    /// Whether the Artist field should take the keyboard. The pane raises it
    /// when the result area has nothing to pick from.
    let focusesArtist: Bool
    let onSearch: () -> Void

    private var isSearchDisabled: Bool {
        switch activeTab {
        case .general:
            searchArtist.isEmpty && searchAlbum.isEmpty
        case .catalogNumber:
            searchCatalog.isEmpty
        case .barcode:
            searchBarcode.isEmpty
        }
    }

    private func submitSearch() {
        guard !isSearchDisabled else { return }
        onSearch()
    }

    /// Shared suggestion pool for Artist and Album. OCR often runs adjacent
    /// lines of cover text together, so an artist name on the spine may just
    /// as well match the Album field.
    private var generalSuggestions: [String] {
        signals?.text.freeText ?? []
    }

    private var catalogSuggestions: [String] {
        signals?.text.catalogValues ?? []
    }

    /// True while core is still producing suggestions. Drives the small
    /// spinner inside each autocomplete field so users know the list is still
    /// filling in.
    private var isScanning: Bool {
        signals?.text.isScanning ?? false
    }

    private var signalFailure: BridgeLookupFailure? {
        signals?.text.failure
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Picker("Search by", selection: $activeTab) {
                    Text("General").tag(SearchTab.general)
                    Text("Catalog #").tag(SearchTab.catalogNumber)
                    Text("Barcode").tag(SearchTab.barcode)
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .controlSize(.small)
                .fixedSize()

                fields

                Button("Search", action: submitSearch)
                    .controlSize(.small)
                    .disabled(isSearchDisabled)
            }
            if let signalFailure {
                Label(
                    signalFailure.badgeLine,
                    systemImage: "exclamationmark.triangle.fill"
                )
                .font(.system(size: 11.5))
                .foregroundStyle(.orange)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .animation(nil, value: activeTab)
    }

    @ViewBuilder
    private var fields: some View {
        switch activeTab {
        case .general:
            AutocompleteTextField(
                text: $searchArtist,
                placeholder: String(localized: "Artist"),
                suggestions: generalSuggestions,
                isLoading: isScanning,
                takesFocus: focusesArtist,
                onSubmit: submitSearch,
            )
            AutocompleteTextField(
                text: $searchAlbum,
                placeholder: String(localized: "Album"),
                suggestions: generalSuggestions,
                isLoading: isScanning,
                onSubmit: submitSearch,
            )
        case .catalogNumber:
            AutocompleteTextField(
                text: $searchCatalog,
                placeholder: String(localized: "e.g. WPCR-80001"),
                suggestions: catalogSuggestions,
                isLoading: isScanning,
                onSubmit: submitSearch,
            )
        case .barcode:
            TextField("e.g. 4943674251780", text: $searchBarcode)
                .textFieldStyle(.roundedBorder)
                .controlSize(.small)
                .onSubmit(submitSearch)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// The fields write back, so a preview holds them.
    private struct ImportSearchFormPreview: View {
        let tab: SearchTab
        @State
        var artist: String = ""
        @State
        var album: String = ""
        @State
        var catalog: String = ""

        var body: some View {
            ImportSearchFormView(
                activeTab: .constant(tab),
                searchArtist: $artist,
                searchAlbum: $album,
                searchCatalog: $catalog,
                searchBarcode: .constant(""),
                signals: PreviewData.settledSignals,
                focusesArtist: false,
                onSearch: {},
            )
        }
    }

    #Preview("General search") {
        ImportSearchFormPreview(
            tab: .general,
            artist: "Artist Name",
            album: "Album Title"
        )
        .frame(width: 660)
        .windowBackground()
    }

    #Preview("Catalog search") {
        ImportSearchFormPreview(tab: .catalogNumber, catalog: "WPCR-80001")
            .frame(width: 660)
            .windowBackground()
    }
#endif
