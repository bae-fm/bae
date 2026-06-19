import AppKit
import SwiftUI

// MARK: - ImportSearchFormView

struct ImportSearchFormView: View {
    @Binding
    var activeTab: SearchTab
    @Binding
    var activeSource: MetadataSource
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

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                sourcePicker
                Picker("", selection: $activeTab) {
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
                        placeholder: "Artist",
                        suggestions: generalSuggestions,
                        isLoading: isScanning,
                        onSubmit: { onSearch() },
                    )
                    AutocompleteTextField(
                        text: $searchAlbum,
                        placeholder: "Album",
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
                        placeholder: "e.g. WPCR-80001",
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
                }
                .allowsHitTesting(false)
        }
    }
}

// MARK: - ImportSearchState

/// Everything `ImportSearchPane` renders from — the identify/results state and
/// the surrounding flags. The editable form fields (search bindings) and the
/// actions stay separate; this is the read-only display state.
struct ImportSearchState {
    let identifyState: IdentifyState
    let showManualSearch: Bool
    let error: String?
    let searchGroups: [ReleaseGroup]
    /// Release id of the pressing whose confirm pane is open, so its row renders
    /// selected.
    let selectedReleaseId: String?
    let isSearching: Bool
    let hasSearched: Bool
    let isImporting: Bool
    let libraryStatuses: [String: LibraryStatus]
    let discogsEnabled: Bool
    let signals: Signals?
    /// The interactive signals toolbar — the pre-shaped badge list. Empty until
    /// the first identify transition; the toolbar is hidden until then.
    let signalsToolbar: SignalsToolbar
}

// MARK: - ImportSearchPane

struct ImportSearchPane: View {
    let state: ImportSearchState
    @Binding
    var activeTab: SearchTab
    @Binding
    var activeSource: MetadataSource
    @Binding
    var searchArtist: String
    @Binding
    var searchAlbum: String
    @Binding
    var searchCatalog: String
    @Binding
    var searchBarcode: String
    let onSearch: () -> Void
    let onOpenSettings: () -> Void
    let onSearchManually: () -> Void
    /// Return from the manual-search form to the auto-identified matches.
    let onViewMatches: () -> Void
    /// Set when the candidate can seed from local files (folder imports always
    /// can). `nil` suppresses the "Add as Unknown" link.
    let onAddAsUnknown: (() -> Void)?
    /// Toggle a signal in the toolbar — include / exclude it from
    /// triangulation. Drops the signal from the in-memory combine step (no
    /// re-fetch) and re-derives the state; the resulting event flows back
    /// through the same channel. The conflict surface's per-signal "Ignore"
    /// links route through here too.
    let onToggleSignal: (ExcludedSignal) -> Void
    /// Re-run the lookups from the toolbar's `Re-run` action.
    let onRerun: () -> Void
    /// A pressing row was picked — the flow opens the docked confirm pane.
    let onSelect: (MetadataResult) -> Void

    /// The auto-identified release group, its library statuses, and per-row
    /// provenance, extracted from the identify state. A `Found` state always
    /// carries exactly one group.
    private var foundResult:
        (
            group: ReleaseGroup, statuses: [String: LibraryStatus],
            provenance: [String: ResultProvenance]
        )?
    {
        guard
            case .found(let group, let statuses, _, _, let provenance) =
                state.identifyState
        else {
            return nil
        }
        return (group, statuses, provenance)
    }

    /// Whether the pipeline is mid-triangulation — drives the body's
    /// "Identifying…" placeholder (the per-signal progress lives in the
    /// toolbar's spinning badges now).
    private var isTriangulating: Bool {
        if case .triangulating = state.identifyState {
            return true
        }
        return false
    }

    /// The signals toolbar, shown across every state once core has emitted a
    /// transition. Hidden until then (empty badge list) and in idle.
    @ViewBuilder
    private var toolbar: some View {
        if !state.signalsToolbar.signals.isEmpty {
            SignalsToolbarView(
                toolbar: state.signalsToolbar,
                onToggle: onToggleSignal,
                onRerun: onRerun,
                onSearchManually: onSearchManually,
                onAddAsUnknown: state.showManualSearch ? nil : onAddAsUnknown,
            )
        }
    }

    var body: some View {
        // Conflict replaces the standard banner + results layout entirely:
        // it stacks per-signal sections so the user can pick a row or
        // toggle a signal. `showManualSearch` skips it — the user
        // explicitly asked for the manual form. `notFoundAnywhere` keeps
        // the flat banner for the truly-empty case.
        if case .conflict(
            let discidResults,
            let discidLibraryStatuses,
            let barcodeResults,
            let barcodeLibraryStatuses,
            let discidSourceLabel,
            let matchedBarcode,
            _
        ) = state.identifyState, !state.showManualSearch {
            conflictView(
                discidResults: discidResults,
                discidLibraryStatuses: discidLibraryStatuses,
                barcodeResults: barcodeResults,
                barcodeLibraryStatuses: barcodeLibraryStatuses,
                discidSourceLabel: discidSourceLabel,
                matchedBarcode: matchedBarcode,
            )
        }
        else {
            normalBody
        }
    }

    @ViewBuilder
    private var normalBody: some View {
        let found = foundResult
        let showingAutoMatches = found != nil && !state.showManualSearch

        VStack(spacing: 0) {
            toolbar

            identifyBanner

            if let error = state.error {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(error)
                }
                .font(.caption)
                .foregroundStyle(.red)
                .padding(.horizontal)
                .padding(.vertical, 6)
            }

            if showingAutoMatches, let found {
                ReleaseGroupListView(
                    groups: [found.group],
                    isImporting: state.isImporting,
                    libraryStatuses: found.statuses,
                    provenance: found.provenance,
                    selectedReleaseId: state.selectedReleaseId,
                    onSelect: onSelect,
                )
            }
            else if isTriangulating, !state.showManualSearch {
                // The toolbar's spinning badges carry the per-signal progress;
                // the body just notes the pipeline is still working.
                ContentUnavailableView(
                    "Identifying\u{2026}",
                    systemImage: "antenna.radiowaves.left.and.right",
                    description: Text("Looking up the signals above."),
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            else {
                // In manual search with an auto-identified match available,
                // offer the way back to it — the toolbar's "Search manually"
                // has no reciprocal otherwise.
                if let found {
                    Button {
                        onViewMatches()
                    } label: {
                        Label(
                            "View automatic matches "
                                + "(\(found.group.pressings.count))",
                            systemImage: "chevron.left",
                        )
                    }
                    .buttonStyle(.link)
                    .font(.callout)
                    .padding(.horizontal, 18)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                ImportSearchFormView(
                    activeTab: $activeTab,
                    activeSource: $activeSource,
                    searchArtist: $searchArtist,
                    searchAlbum: $searchAlbum,
                    searchCatalog: $searchCatalog,
                    searchBarcode: $searchBarcode,
                    discogsEnabled: state.discogsEnabled,
                    signals: state.signals,
                    onSearch: onSearch,
                    onOpenSettings: onOpenSettings,
                )
                Divider()

                if state.isSearching {
                    ProgressView("Searching...")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                else if !state.hasSearched {
                    ContentUnavailableView(
                        "No results",
                        systemImage: "magnifyingglass",
                        description: Text(
                            "Search MusicBrainz or Discogs to find metadata"
                        ),
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                else if state.searchGroups.isEmpty {
                    ContentUnavailableView(
                        "No matches found",
                        systemImage: "magnifyingglass",
                        description: Text(
                            "Try different search terms or another source"
                        ),
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                else {
                    ReleaseGroupListView(
                        groups: state.searchGroups,
                        isImporting: state.isImporting,
                        libraryStatuses: state.libraryStatuses,
                        selectedReleaseId: state.selectedReleaseId,
                        onSelect: onSelect,
                    )
                }
            }
        }
        .padding(.top, 6)
    }

    @ViewBuilder
    private var identifyBanner: some View {
        // The toolbar above owns the global escapes (Search manually / Skip
        // identifying / Re-run), so the banner carries only the status copy for
        // the states that need it. Error has no toolbar, so it keeps its own
        // escape.
        switch state.identifyState {
        case .triangulating, .found, .idle:
            // Triangulation is covered by the toolbar's spinning badges and the
            // body placeholder; a found match speaks for itself through the
            // toolbar signals and the release-group card below. Neither needs a
            // banner.
            EmptyView()
        case .notFoundAnywhere:
            HStack(spacing: 8) {
                Image(systemName: "info.circle.fill")
                    .foregroundStyle(.orange)
                Text("No automatic matches found")
                    .font(.callout)
                discIdInfoIcon
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .overlay(alignment: .bottom) {
                Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
            }
        case .manualOnly:
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass.circle.fill")
                    .foregroundStyle(.secondary)
                Text("No identifying signals yet — search manually")
                    .font(.callout)
                discIdInfoIcon
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .overlay(alignment: .bottom) {
                Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
            }
        case .conflict:
            // The full per-signal surface renders from `body` when not in
            // manual-search mode. This banner is the manual-search-mode
            // status line; the toolbar carries the escapes.
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.octagon.fill")
                    .foregroundStyle(Theme.accent)
                Text("Signals disagree on identity")
                    .font(.callout)
                discIdInfoIcon
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .overlay(alignment: .bottom) {
                Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
            }
        }
    }

    /// Conflict surface. Renders when both signals returned releases but
    /// they don't agree on a single group (or the intersection was empty).
    /// One section per signal that produced results, stacked vertically;
    /// the user can pick a row directly or exclude a signal (via its section
    /// "Ignore" link or the toolbar toggle) to re-derive without it.
    @ViewBuilder
    private func conflictView(
        discidResults: [MetadataResult],
        discidLibraryStatuses: [String: LibraryStatus],
        barcodeResults: [MetadataResult],
        barcodeLibraryStatuses: [String: LibraryStatus],
        discidSourceLabel: String?,
        matchedBarcode: String?,
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            toolbar

            conflictBannerLarge

            if let error = state.error {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(error)
                }
                .font(.caption)
                .foregroundStyle(.red)
                .padding(.horizontal, 18)
                .padding(.vertical, 6)
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    // `discidSourceLabel` is non-nil exactly when the
                    // disc-id side has results (core sets them together),
                    // so the combined guard never hides a populated section.
                    if !discidResults.isEmpty, let discidSourceLabel {
                        conflictSection(
                            signal: .disc,
                            title: "DiscID",
                            subtitle: discidSectionSubtitle(
                                count: discidResults.count,
                                sourceLabel: discidSourceLabel
                            ),
                            results: discidResults,
                            libraryStatuses: discidLibraryStatuses,
                        )
                    }
                    if !barcodeResults.isEmpty {
                        conflictSection(
                            signal: .barcode,
                            title: "Barcode",
                            subtitle: barcodeSectionSubtitle(
                                count: barcodeResults.count,
                                matchedBarcode: matchedBarcode
                            ),
                            results: barcodeResults,
                            libraryStatuses: barcodeLibraryStatuses,
                        )
                    }
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 16)
            }
        }
    }

    /// "Signals disagree on identity" banner — warm-amber tint, two-line
    /// copy that names the choice the user has to make. Replaces the
    /// thin caption-style banner the conflict view used to lead with.
    private var conflictBannerLarge: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.octagon.fill")
                .font(.callout)
                .foregroundStyle(Theme.accent)
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 2) {
                Text("Signals disagree on identity")
                    .font(.subheadline)
                    .fontWeight(.semibold)
                Text(
                    "Pick the release you have, or dismiss the signal you trust less."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            discIdInfoIcon
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .background(Theme.accent.opacity(0.12))
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
        }
    }

    /// One per-signal section in the conflict surface: uppercase tracked
    /// title + subtitle on a divider line, an "Ignore" link aligned
    /// right, and pressing-shaped rows below.
    @ViewBuilder
    private func conflictSection(
        signal: ExcludedSignal,
        title: String,
        subtitle: AttributedString,
        results: [MetadataResult],
        libraryStatuses: [String: LibraryStatus],
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(title)
                    .font(.caption2)
                    .fontWeight(.bold)
                    .textCase(.uppercase)
                    .tracking(1.4)
                    .foregroundStyle(.secondary)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Spacer()
                Button(ignoreButtonLabel(signal: signal)) {
                    onToggleSignal(signal)
                }
                .buttonStyle(.link)
                .font(.caption)
                .disabled(state.isImporting)
            }
            .padding(.top, 8)
            .padding(.bottom, 6)
            .overlay(alignment: .bottom) {
                Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
            }
            VStack(spacing: 0) {
                ForEach(results, id: \.releaseId) { result in
                    ImportSearchResultRow(
                        result: result,
                        isImporting: state.isImporting,
                        libraryStatus: libraryStatuses[result.releaseId],
                        isSelected: result.releaseId == state.selectedReleaseId,
                        onSelect: { onSelect(result) },
                    )
                    Rectangle().fill(.white.opacity(0.05)).frame(height: 1)
                }
            }
        }
    }

    /// Disc-ID section subtitle. The source the disc-id lookup consulted
    /// is supplied by core (`discidSourceLabel`) rather than hardcoded,
    /// so the UI doesn't bake in which database disc-id uses.
    private func discidSectionSubtitle(count: Int, sourceLabel: String)
        -> AttributedString
    {
        let releaseWord = count == 1 ? "release" : "releases"
        return AttributedString(
            "matched \(count) \(releaseWord) on \(sourceLabel)"
        )
    }

    /// Barcode section subtitle — surface the matched value (monospaced
    /// inline) so the user can correlate against the artwork that
    /// produced it. Falls back to a value-less label when the matched
    /// barcode wasn't preserved.
    private func barcodeSectionSubtitle(
        count: Int,
        matchedBarcode: String?
    ) -> AttributedString {
        let releaseWord = count == 1 ? "release" : "releases"
        var subtitle = AttributedString("matched \(count) \(releaseWord)")
        if let barcode = matchedBarcode, !barcode.isEmpty {
            subtitle += AttributedString(" · ")
            var mono = AttributedString(barcode)
            mono.font = .system(.caption, design: .monospaced)
            subtitle += mono
        }
        return subtitle
    }

    private func ignoreButtonLabel(signal: ExcludedSignal) -> String {
        switch signal {
        case .disc: "Ignore DiscID"
        case .barcode: "Ignore Barcode"
        case .catalog: "Ignore Catalog"
        }
    }

    private var discIdInfoIcon: some View {
        InfoTip(
            text: "Uses track layout to find perfect matches on MusicBrainz.",
            learnMoreURL: URL(
                string: "https://bae.fm/importing/local-files#identify"
            ),
            width: 260,
        )
    }
}

// MARK: - ImportSearchResultRow

// MARK: - Source Segmented Control

/// NSSegmentedControl wrapper that supports disabling individual segments.
struct SourceSegmentedControl: NSViewRepresentable {
    @Binding
    var selection: MetadataSource
    let discogsEnabled: Bool
    @Binding
    var showDiscogsInfo: Bool
    @Binding
    var hoverTask: DispatchWorkItem?

    func makeNSView(context: Context) -> TrackingSegmentedControl {
        let control = TrackingSegmentedControl(
            labels: ["MusicBrainz", "Discogs"],
            trackingMode: .selectOne,
            target: context.coordinator,
            action: #selector(Coordinator.segmentChanged(_:))
        )
        control.segmentStyle = .rounded
        control.selectedSegment = selection == .musicBrainz ? 0 : 1
        control.setEnabled(discogsEnabled, forSegment: 1)
        control.setWidth(100, forSegment: 0)
        control.setWidth(100, forSegment: 1)
        control.coordinator = context.coordinator
        return control
    }

    func updateNSView(_ control: TrackingSegmentedControl, context: Context) {
        control.selectedSegment = selection == .musicBrainz ? 0 : 1
        control.setEnabled(discogsEnabled, forSegment: 1)
        context.coordinator.parent = self
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    class TrackingSegmentedControl: NSSegmentedControl {
        weak var coordinator: Coordinator?

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            for area in trackingAreas {
                removeTrackingArea(area)
            }
            addTrackingArea(
                NSTrackingArea(
                    rect: bounds,
                    options: [
                        .mouseEnteredAndExited, .mouseMoved, .activeInActiveApp,
                    ],
                    owner: self,
                    userInfo: nil,
                )
            )
        }

        override func mouseEntered(with event: NSEvent) {
            coordinator?.handleMouse(event, entered: true)
        }

        override func mouseMoved(with event: NSEvent) {
            coordinator?.handleMouseMoved(event, in: self)
        }

        override func mouseExited(with event: NSEvent) {
            coordinator?.handleMouse(event, entered: false)
        }
    }

    @MainActor
    class Coordinator: NSObject {
        var parent: SourceSegmentedControl
        private var isOverDiscogs = false

        init(parent: SourceSegmentedControl) {
            self.parent = parent
        }

        @objc
        func segmentChanged(_ sender: NSSegmentedControl) {
            parent.selection =
                sender.selectedSegment == 0 ? .musicBrainz : .discogs
        }

        func handleMouse(_: NSEvent, entered: Bool) {
            guard !parent.discogsEnabled else {
                return
            }
            if !entered, isOverDiscogs {
                isOverDiscogs = false
                showPopoverDelayed(false)
            }
        }

        func handleMouseMoved(_ event: NSEvent, in control: NSSegmentedControl)
        {
            guard !parent.discogsEnabled else {
                return
            }
            let location = control.convert(event.locationInWindow, from: nil)
            let midX = control.bounds.width / 2
            let overDiscogs = location.x > midX

            if overDiscogs, !isOverDiscogs {
                isOverDiscogs = true
                showPopoverDelayed(true)
            }
            else if !overDiscogs, isOverDiscogs {
                isOverDiscogs = false
                showPopoverDelayed(false)
            }
        }

        private func showPopoverDelayed(_ show: Bool) {
            parent.hoverTask?.cancel()
            let task = DispatchWorkItem { [weak self] in
                self?.parent.showDiscogsInfo = show
            }
            parent.hoverTask = task
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: task)
        }
    }
}

/// Popover content shown when hovering over a disabled Discogs segment.
struct DiscogsKeyPopover: View {
    @Binding
    var isPresented: Bool
    @Binding
    var hoverTask: DispatchWorkItem?
    let onOpenSettings: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(
                "[Discogs](https://www.discogs.com) is a music database with detailed release info — labels, catalog numbers, pressing variants, and more."
            )
            .font(.callout)
            Text(
                "To search Discogs, [get your free API key](https://www.discogs.com/settings/developers) and add it in settings."
            )
            .font(.callout)
            Button("Open Settings") {
                isPresented = false
                onOpenSettings()
            }
            .font(.callout)
        }
        .padding(10)
        .frame(width: 280)
        .onHover { hovering in
            hoverTask?.cancel()
            if !hovering {
                let task = DispatchWorkItem { isPresented = false }
                hoverTask = task
                DispatchQueue.main.asyncAfter(
                    deadline: .now() + 0.3,
                    execute: task
                )
            }
        }
    }
}

// MARK: - Previews

#if DEBUG
    extension ImportSearchPane {
        /// Preview builder — fixes the form bindings and action callbacks to inert
        /// defaults so a preview states only the result situation it exercises.
        static func preview(
            state: ImportSearchState,
            searchArtist: String = "",
            searchAlbum: String = "",
        ) -> ImportSearchPane {
            ImportSearchPane(
                state: state,
                activeTab: .constant(.general),
                activeSource: .constant(.musicBrainz),
                searchArtist: .constant(searchArtist),
                searchAlbum: .constant(searchAlbum),
                searchCatalog: .constant(""),
                searchBarcode: .constant(""),
                onSearch: {},
                onOpenSettings: {},
                onSearchManually: {},
                onViewMatches: {},
                onAddAsUnknown: {},
                onToggleSignal: { _ in },
                onRerun: {},
                onSelect: { _ in },
            )
        }
    }
#endif

#Preview("Main Pane - Exact Matches") {
    ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
        .frame(width: 700, height: 600)
        .background(Theme.background)
        .preferredColorScheme(.dark)
        .importPreviewEnvironment()
}

#Preview("Main Pane - Manual Search") {
    ImportSearchPane.preview(
        state: PreviewData.searchStateManual,
        searchArtist: "Artist Name",
        searchAlbum: "Album Title One",
    )
    .frame(width: 700, height: 600)
    .background(Theme.background)
    .preferredColorScheme(.dark)
    .importPreviewEnvironment()
}

#Preview("Main Pane - Conflict") {
    ImportSearchPane.preview(state: PreviewData.searchStateConflict)
        .frame(width: 700, height: 600)
        .background(Theme.background)
        .preferredColorScheme(.dark)
        .importPreviewEnvironment()
}

#Preview("Source Picker - Discogs Disabled") {
    SourcePickerPreview(hasKey: false)
        .frame(width: 500, height: 100)
        .preferredColorScheme(.dark)
}

private struct SourcePickerPreview: View {
    let hasKey: Bool
    @State
    private var source: MetadataSource = .musicBrainz
    @State
    private var showPopover = false
    @State
    private var hoverTask: DispatchWorkItem?

    var body: some View {
        SourceSegmentedControl(
            selection: $source,
            discogsEnabled: hasKey,
            showDiscogsInfo: $showPopover,
            hoverTask: $hoverTask,
        )
        .frame(width: 200)
        .overlay(alignment: .bottomTrailing) {
            Color.clear
                .frame(width: 100, height: 1)
                .popover(isPresented: $showPopover, arrowEdge: .bottom) {
                    DiscogsKeyPopover(
                        isPresented: $showPopover,
                        hoverTask: $hoverTask,
                        onOpenSettings: {},
                    )
                }
                .allowsHitTesting(false)
        }
        .padding()
    }
}
