import BaeKit
import SwiftUI

/// The manual-search form: source picker, tab selector (General / Catalog # /
/// Barcode), and the tab's fields with autocomplete seeded from the candidate's
/// scanned text signals. Submits through `onSearch`.
struct ImportSearchFormView: View {
    @Binding
    var activeTab: SearchTab
    @Binding
    var activeSource: BridgeMetadataSource
    @Binding
    var searchArtist: String
    @Binding
    var searchAlbum: String
    @Binding
    var searchCatalog: String
    @Binding
    var searchBarcode: String
    let discogsEnabled: Bool
    let signals: Signals?
    let onSearch: () -> Void
    let onOpenSettings: () -> Void

    @State
    private var showDiscogsKeyInfo: Bool = false
    @State
    private var discogsKeyHoverTask: DispatchWorkItem?

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
    /// spinner inside each autocomplete dropdown so users know the list is
    /// still filling in — cheap visual signal that the pool is in motion.
    private var isScanning: Bool {
        signals?.text.isScanning ?? false
    }

    private var signalFailure: BridgeLookupFailure? {
        signals?.text.failure
    }

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                sourcePicker
                Picker("Search by", selection: $activeTab) {
                    Text("General").tag(SearchTab.general)
                    Text("Catalog #").tag(SearchTab.catalogNumber)
                    Text("Barcode").tag(SearchTab.barcode)
                }
                .pickerStyle(.segmented)
                .controlSize(.small)
            }

            switch activeTab {
            case .general:
                HStack {
                    AutocompleteTextField(
                        text: $searchArtist,
                        placeholder: String(localized: "Artist"),
                        suggestions: generalSuggestions,
                        isLoading: isScanning,
                        onSubmit: { onSearch() },
                    )
                    AutocompleteTextField(
                        text: $searchAlbum,
                        placeholder: String(localized: "Album"),
                        suggestions: generalSuggestions,
                        isLoading: isScanning,
                        onSubmit: { onSearch() },
                    )
                    Button("Search") { onSearch() }
                        .disabled(isSearchDisabled)
                }
            case .catalogNumber:
                HStack {
                    AutocompleteTextField(
                        text: $searchCatalog,
                        placeholder: String(localized: "e.g. WPCR-80001"),
                        suggestions: catalogSuggestions,
                        isLoading: isScanning,
                        onSubmit: { onSearch() },
                    )
                    Button("Search") { onSearch() }
                        .disabled(isSearchDisabled)
                }
            case .barcode:
                HStack {
                    TextField("e.g. 4943674251780", text: $searchBarcode)
                        .textFieldStyle(.roundedBorder)
                    Button("Search") { onSearch() }
                        .disabled(isSearchDisabled)
                }
                .onSubmit { onSearch() }
            }

            if let signalFailure {
                Label(
                    signalFailure.badgeLine,
                    systemImage: "exclamationmark.triangle.fill"
                )
                .font(.system(size: 11.5))
                .foregroundStyle(.orange)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding()
        .animation(nil, value: activeTab)
    }

    private var sourcePicker: some View {
        SourceSegmentedControl(
            selection: $activeSource,
            discogsEnabled: discogsEnabled,
            showDiscogsInfo: $showDiscogsKeyInfo,
            hoverTask: $discogsKeyHoverTask,
        )
        .frame(width: 200)
        .overlay(alignment: .bottomTrailing) {
            Color.clear
                .frame(width: 100, height: 1)
                .popover(isPresented: $showDiscogsKeyInfo, arrowEdge: .bottom) {
                    DiscogsKeyPopover(
                        isPresented: $showDiscogsKeyInfo,
                        hoverTask: $discogsKeyHoverTask,
                        onOpenSettings: { onOpenSettings() },
                    )
                    .popoverEntrance(anchor: .top)
                    .background { PopoverBehavior() }
                }
                .allowsHitTesting(false)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("General search") {
        ImportSearchFormView(
            activeTab: .constant(.general),
            activeSource: .constant(.musicBrainz),
            searchArtist: .constant("Artist Name"),
            searchAlbum: .constant("Album Title"),
            searchCatalog: .constant(""),
            searchBarcode: .constant(""),
            discogsEnabled: true,
            signals: PreviewData.settledSignals,
            onSearch: {},
            onOpenSettings: {},
        )
        .frame(width: 560)
        .windowBackground()
    }

    #Preview("Catalog search") {
        ImportSearchFormView(
            activeTab: .constant(.catalogNumber),
            activeSource: .constant(.musicBrainz),
            searchArtist: .constant(""),
            searchAlbum: .constant(""),
            searchCatalog: .constant("WPCR-80001"),
            searchBarcode: .constant(""),
            discogsEnabled: false,
            signals: PreviewData.settledSignals,
            onSearch: {},
            onOpenSettings: {},
        )
        .frame(width: 560)
        .windowBackground()
    }
#endif
