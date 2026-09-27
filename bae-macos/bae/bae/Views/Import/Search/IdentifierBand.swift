import BaeKit
import SwiftUI

/// The run as one wrapping band of chips — Disc ID, Barcode, Catalog #, Title,
/// then the catalog numbers that rank the answers, then the unused numbers
/// folded behind their count — each with every provider's answer.
struct IdentifierBand: View {
    let run: BridgeIdentifyRun
    /// The numbers an offered release carries; empty while the run is going.
    let catalogAgreements: [BridgeCatalogAgreement]
    /// Leave an identifier out of the run, or ask about it again.
    let onToggleLookup: (LookupToggle) -> Void
    /// Count a catalog number the folder states, or stop counting it.
    let onToggleCatalogAgreement: (String) -> Void
    /// Re-ask only the lookups that failed.
    let onRetryFailed: () -> Void
    /// Search by these words instead of the draft's; both blank goes back to
    /// the draft's.
    let onEditTitleSearch: (_ album: String, _ artist: String) -> Void

    /// Whether the catalog numbers nothing confirmed are shown.
    @State
    private var showsCatalogCandidates = false

    var body: some View {
        FlowLayout(spacing: ThemeSpace.compact) {
            discIdChip
            barcodeChips
            catalogChips
            titleChip
            ForEach(catalogAgreements, id: \.value) { agreement in
                CatalogAgreementChip(
                    agreement: agreement,
                    onToggle: { onToggleCatalogAgreement(agreement.value) }
                )
            }
            if showsCatalogCandidates {
                ForEach(catalogCandidates, id: \.value) { candidate in
                    CatalogCandidateChip(
                        candidate: candidate,
                        onActivate: {
                            onToggleLookup(.catalog(candidate.value))
                        }
                    )
                }
            }
            if !catalogCandidates.isEmpty {
                CatalogCandidatesDisclosure(
                    count: catalogCandidates.count,
                    isExpanded: $showsCatalogCandidates
                )
            }
            if isScanning {
                ScanningChip()
            }
        }
        .padding(.vertical, ThemeSpace.related)
        .padding(.horizontal, ThemeSpace.group)
    }

    // MARK: - Disc ID

    /// One chip with MusicBrainz's capsule, the only catalog that answers disc
    /// IDs; a read disc ID toggles in and out of the run.
    @ViewBuilder
    private var discIdChip: some View {
        let label = SignalBadgeStyle.label(for: BridgeSignalKind.discId)
        switch run.discId {
        case .reading:
            IdentifierChip(label: label) { ChipSpinner() }
                .help("Reading…")
        case .absent:
            IdentifierChip(label: label) { IdentifierDash() }
                .help("No LOG or CUE in the folder")
        // The CUE's audio is at a rate no CD holds, so no disc ID was read.
        case .notCdAudio(let sampleRateHz):
            IdentifierChip(label: label) { IdentifierDash() }
                .help(
                    sampleRateHz.map { sampleRateHz in
                        String(
                            localized:
                                "Not read: the audio is \(sampleRateText(hz: Double(sampleRateHz))), which no CD holds"
                        )
                    } ?? ""
                )
        case .readFailed(let failure):
            IdentifierChip(label: label) { IdentifierWarning() }
                .help(
                    String(
                        localized:
                            "Couldn't read the disc layout: \(failure.briefLine)"
                    )
                )
        // Left out: clicking the chip asks about it again.
        case .read(let discId, .notAsked(reason: .leftOut)):
            discIdButton(label: label, discId: discId, lookup: nil)
        // Only Settings can switch on the catalog that answers disc IDs.
        case .read(let discId, .notAsked(reason: .noCatalog)):
            IdentifierChip(label: label, value: discId) {
                IdentifierDash()
            }
        case .read(let discId, let lookup):
            discIdButton(label: label, discId: discId, lookup: lookup)
        }
    }

    /// The disc ID as a toggle; `lookup` is `nil` when the person left it out.
    private func discIdButton(
        label: String,
        discId: String,
        lookup: BridgeLookupState?
    ) -> some View {
        Button {
            onToggleLookup(.discId)
        } label: {
            IdentifierChip(
                label: label,
                value: discId,
                style: lookup == nil ? .outlined : .filled
            ) {
                if let lookup {
                    ProviderCapsule(
                        source: .musicBrainz,
                        lookup: lookup,
                        onRetry: onRetryFailed
                    )
                }
            }
        }
        .buttonStyle(.plain)
        .help(
            lookup == nil
                ? "Ask about this disc ID again"
                : "Leave this disc ID out of the run"
        )
    }

    // MARK: - Barcode

    /// A chip per code the folder carries, each toggling in and out of the run.
    @ViewBuilder
    private var barcodeChips: some View {
        let label = SignalBadgeStyle.label(for: BridgeSignalKind.barcode)
        switch run.barcode {
        case .absent:
            IdentifierChip(label: label) { IdentifierDash() }
                .help("No barcode source")
        // The art may carry a code, but reading it is switched off.
        case .coverArtOff:
            IdentifierChip(label: label) { IdentifierOff() }
                .help(
                    "Cover art isn't read: switched off in Import settings"
                )
        case .noCodes:
            IdentifierChip(label: label) { IdentifierDash() }
                .help("No barcode on the artwork")
        case .scanFailed(let failure):
            IdentifierChip(label: label) { IdentifierWarning() }
                .help(
                    String(
                        localized:
                            "Couldn't read the barcodes: \(failure.briefLine)"
                    )
                )
        case .rows(_, let rows):
            ForEach(rows, id: \.value) { row in
                Button {
                    onToggleLookup(.barcode(row.value))
                } label: {
                    IdentifierChip(
                        label: label,
                        value: row.value,
                        style: row.excluded ? .outlined : .filled
                    ) {
                        // A left-out code has no answers to show.
                        if !row.excluded {
                            capsules(row.cells)
                        }
                    }
                }
                .buttonStyle(.plain)
                .help(
                    row.excluded
                        ? "Ask about this barcode again"
                        : "Leave this barcode out of the run"
                )
            }
        }
    }

    // MARK: - Catalog #

    /// A chip per number the run looks up; clicking it takes the number out.
    @ViewBuilder
    private var catalogChips: some View {
        let label = String(localized: "Catalog #")
        switch run.catalog {
        case .noneFound:
            IdentifierChip(label: label) { IdentifierDash() }
                .help("None found")
        case .coverArtOff:
            IdentifierChip(label: label) { IdentifierOff() }
                .help(
                    "Cover art isn't read: switched off in Import settings"
                )
        case .numbers(_, let rows, _):
            ForEach(rows, id: \.value) { row in
                Button {
                    onToggleLookup(.catalog(row.value))
                } label: {
                    IdentifierChip(
                        label: label,
                        value: row.value
                    ) {
                        capsules(row.cells)
                    }
                }
                .buttonStyle(.plain)
                .help("Take this catalog number out of the run")
            }
        }
    }

    /// Every provider's answer about one value, in the run's provider order.
    @ViewBuilder
    private func capsules(_ cells: [BridgeProviderCell]) -> some View {
        ForEach(cells, id: \.source) { cell in
            ProviderCapsule(
                source: cell.source,
                lookup: cell.lookup,
                onRetry: onRetryFailed
            )
        }
    }

    /// The numbers found that the run doesn't look up and no release carries.
    private var catalogCandidates: [BridgeCatalogCandidate] {
        guard case .numbers(_, _, let candidates) = run.catalog else {
            return []
        }
        return candidates
    }

    /// Whether the artwork is still being read, so more chips may come.
    private var isScanning: Bool {
        if case .rows(scanning: true, rows: _) = run.barcode { return true }
        if case .numbers(scanning: true, rows: _, candidates: _) = run.catalog {
            return true
        }
        return false
    }
}

extension IdentifierBand {
    // MARK: - Title

    /// The words the run searches by when the identifiers name nothing, with
    /// each provider's answer; editing them searches again.
    @ViewBuilder
    private var titleChip: some View {
        switch run.search {
        case .notNeeded:
            EmptyView()
        case .notAsked(reason: .switchedOff):
            IdentifierChip(label: String(localized: "Title")) {
                IdentifierOff()
            }
            .help("Searching by title is switched off in Import settings")
        // Never reached: a title is never left out, and every catalog searches.
        case .notAsked(reason: .leftOut), .notAsked(reason: .noCatalog):
            EmptyView()
        case .noTitle:
            TitleSearchChip(album: "", artist: "", onCommit: onEditTitleSearch)
            {
                EmptyView()
            }
            .help("No title to search by")
        // Not editable until the run reaches the search.
        case .waiting(let album, let artist):
            TitleSearchChip(
                album: album,
                artist: artist,
                isWaiting: true,
                onCommit: onEditTitleSearch
            ) {
                ChipSpinner()
            }
        case .searched(let album, let artist, let cells):
            TitleSearchChip(
                album: album,
                artist: artist,
                onCommit: onEditTitleSearch
            ) {
                capsules(cells)
            }
        }
    }
}

/// The catalog numbers nothing confirmed, folded behind their count.
private struct CatalogCandidatesDisclosure: View {
    let count: Int
    @Binding
    var isExpanded: Bool

    var body: some View {
        Button {
            isExpanded.toggle()
        } label: {
            Text(
                isExpanded
                    ? String(localized: "Fewer")
                    : String(localized: "+\(count) more")
            )
            .themeText(.chip)
            .foregroundStyle(.secondary)
            .padding(.horizontal, ThemeSpace.compact)
            .padding(.vertical, ThemeSpace.line)
            .background(
                Theme.hover,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
        }
        .buttonStyle(.plain)
        .help("Catalog numbers found in the folder that no release confirms")
    }
}

/// The artist and title fields the run searches by; leaving a changed field
/// commits both.
private struct TitleSearchChip<Trailing: View>: View {
    let album: String
    let artist: String
    /// The run has not reached the search yet, so the fields are read-only.
    let isWaiting: Bool
    let onCommit: (_ album: String, _ artist: String) -> Void
    let trailing: Trailing

    @State
    private var albumText: String
    @State
    private var artistText: String
    @FocusState
    private var focused: Field?

    private enum Field {
        case album
        case artist
    }

    init(
        album: String,
        artist: String,
        isWaiting: Bool = false,
        onCommit: @escaping (_ album: String, _ artist: String) -> Void,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.album = album
        self.artist = artist
        self.isWaiting = isWaiting
        self.onCommit = onCommit
        self.trailing = trailing()
        _albumText = State(initialValue: album)
        _artistText = State(initialValue: artist)
    }

    var body: some View {
        IdentifierChip(label: String(localized: "Artist")) {
            field("Artist", text: $artistText, field: .artist)
            IdentifierLabel(text: String(localized: "Title"))
            field("Title", text: $albumText, field: .album)
            trailing
        }
        // A new run's words replace what was typed.
        .onChange(of: album) { _, now in albumText = now }
        .onChange(of: artist) { _, now in artistText = now }
        // Leaving a field searches, including moving to the other one.
        .onChange(of: focused) { was, now in
            if was != nil, was != now { commit() }
        }
    }

    /// A field with no placeholder; `name` is what accessibility reads.
    private func field(
        _ name: LocalizedStringKey,
        text: Binding<String>,
        field: Field
    ) -> some View {
        TextField(name, text: text, prompt: Text(verbatim: ""))
            .textFieldStyle(.plain)
            .themeText(.mono)
            .foregroundStyle(.secondary)
            .focused($focused, equals: field)
            .disabled(isWaiting)
            .onSubmit { focused = nil }
            .frame(minWidth: 60, idealWidth: 140)
            .fixedSize(horizontal: true, vertical: false)
    }

    private func commit() {
        if albumText == album && artistText == artist { return }
        onCommit(albumText, artistText)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Run in flight") {
        IdentifierBand(
            run: PreviewData.identifyRunInFlight,
            catalogAgreements: [],
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
            onEditTitleSearch: { _, _ in },
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    /// One source asked, which does not answer disc IDs.
    #Preview("Run on one source") {
        IdentifierBand(
            run: PreviewData.identifyRunOneSource,
            catalogAgreements: [],
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
            onEditTitleSearch: { _, _ in },
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    #Preview("Run starting") {
        IdentifierBand(
            run: PreviewData.identifyRunStarting,
            catalogAgreements: [],
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
            onEditTitleSearch: { _, _ in },
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    #Preview("A provider failed") {
        IdentifierBand(
            run: PreviewData.identifyRunProviderFailed,
            catalogAgreements: [],
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
            onEditTitleSearch: { _, _ in },
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    /// "A provider failed" with its catalog number taken back out.
    #Preview("A catalog number waiting to be used") {
        IdentifierBand(
            run: PreviewData.identifyRunCatalogWaiting,
            catalogAgreements: PreviewData.catalogAgreements,
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
            onEditTitleSearch: { _, _ in },
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    /// A disc ID left out, then two barcodes both asked about, then one of
    /// them left out.
    #Preview("Identifiers left out of the run") {
        VStack(alignment: .leading, spacing: 0) {
            IdentifierBand(
                run: PreviewData.identifyRunDiscIdLeftOut,
                catalogAgreements: [],
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
                onEditTitleSearch: { _, _ in },
            )
            IdentifierBand(
                run: PreviewData.identifyRunBothBarcodesAsked,
                catalogAgreements: [],
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
                onEditTitleSearch: { _, _ in },
            )
            IdentifierBand(
                run: PreviewData.identifyRunBarcodeLeftOut,
                catalogAgreements: [],
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
                onEditTitleSearch: { _, _ in },
            )
        }
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    #Preview("Steps switched off") {
        IdentifierBand(
            run: PreviewData.identifyRunStepsOff,
            catalogAgreements: [],
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
            onEditTitleSearch: { _, _ in },
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    #Preview("Nothing found") {
        IdentifierBand(
            run: PreviewData.identifyRunNothingFound,
            catalogAgreements: [],
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
            onEditTitleSearch: { _, _ in },
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }
#endif
