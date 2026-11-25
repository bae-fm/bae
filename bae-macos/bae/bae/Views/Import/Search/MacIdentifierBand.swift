import BaeKit
import SwiftUI

/// The run as one wrapping band of chips — Disc ID, Barcode, Catalog #, ISRC,
/// Title, then the catalog numbers nothing confirmed folded behind their
/// count — each with every provider's answer.
struct MacIdentifierBand: View {
    let run: BridgeIdentifyRun
    /// Leave an identifier out of the run, or ask about it again.
    let onToggleLookup: (BridgeLookupChoiceEdit) -> Void
    /// Strike a catalog number in effect out of the run, or put a struck one
    /// back.
    let onToggleCatalogAgreement: (String) -> Void
    /// Re-ask only the lookups that failed.
    let onRetryFailed: () -> Void
    /// Search by these words instead of the draft's; both blank goes back to
    /// the draft's.
    let onEditTitleSearch: (_ album: String, _ artist: String) -> Void

    /// Whether the catalog numbers nothing confirmed are listed; the pane
    /// owns it, so it stays when the band draws again.
    @Binding
    var listsCatalogCandidates: Bool

    var body: some View {
        FlowLayout(spacing: ThemeSpace.compact) {
            discIdChips
            barcodeChips
            catalogChips
            isrcChip
            titleChip
            catalogCandidates
            if let wait = ChipWait(readingArtworkOf: run) {
                ScanningChip(wait: wait)
            }
        }
    }

    // MARK: - Disc ID

    /// One chip per Disc ID with MusicBrainz's capsule, the only catalog that
    /// answers Disc IDs; a read Disc ID toggles the Disc IDs in and out of the
    /// run.
    @ViewBuilder
    private var discIdChips: some View {
        let label = SignalBadgeStyle.label(for: SignalBadgeStyle.Signal.discId)
        switch run.discId {
        case .reading:
            MacIdentifierChip(label: label) { ChipWait(waitingOn: run.discId) }
        case .absent:
            MacIdentifierChip(label: label)
                .help("No LOG or CUE in the folder")
        // The CUE's audio is at a rate no CD holds, so no Disc ID was read.
        case .notCdAudio(let sampleRateHz):
            MacIdentifierChip(label: label)
                .help(
                    sampleRateHz.map { sampleRateHz in
                        String(
                            localized:
                                "Not read: the audio is \(sampleRateText(hz: Double(sampleRateHz))), which no CD holds"
                        )
                    } ?? ""
                )
        case .read(let discs):
            ForEach(discs, id: \.discId) { disc in
                discIdChip(label: label, disc: disc)
            }
        }
    }

    @ViewBuilder
    private func discIdChip(label: String, disc: BridgeDiscIdRow) -> some View {
        switch disc.lookup {
        // Left out: clicking the chip asks about it again.
        case .notAsked(reason: .leftOut):
            discIdButton(label: label, discId: disc.discId, lookup: nil)
        // Only Settings can switch on the catalog that answers Disc IDs.
        case .notAsked(reason: .noCatalog):
            MacIdentifierChip(label: label, value: disc.discId)
        case let lookup:
            discIdButton(label: label, discId: disc.discId, lookup: lookup)
        }
    }

    /// The Disc ID as a toggle; `lookup` is `nil` when the person left it out.
    private func discIdButton(
        label: String,
        discId: String,
        lookup: BridgeLookupState?
    ) -> some View {
        MacIdentifierChip(
            label: label,
            value: discId,
            style: lookup == nil ? .outlined : .filled,
            onPress: { onToggleLookup(.toggleDiscId) }
        ) {
            if let lookup {
                ProviderCapsule(
                    source: .musicBrainz,
                    lookup: lookup,
                    onRetry: onRetryFailed
                )
            }
        }
        .help(
            lookup == nil
                ? "Ask about this Disc ID again"
                : "Leave this Disc ID out of the run"
        )
    }

    // MARK: - Barcode

    /// A chip per code the folder carries, each toggling in and out of the run.
    @ViewBuilder
    private var barcodeChips: some View {
        let label = SignalBadgeStyle.label(for: SignalBadgeStyle.Signal.barcode)
        switch run.barcode {
        case .absent:
            MacIdentifierChip(label: label)
                .help("No barcode source")
        case .noCodes:
            MacIdentifierChip(label: label)
                .help("No barcode on the artwork")
        case .rows(let rows):
            ForEach(rows, id: \.value) { row in
                MacIdentifierChip(
                    label: label,
                    value: row.value,
                    style: row.excluded ? .outlined : .filled,
                    onPress: { onToggleLookup(.toggleBarcode(code: row.value)) }
                ) {
                    // A left-out code has no answers to show.
                    if !row.excluded {
                        capsules(row.cells)
                    }
                }
                .help(
                    row.excluded
                        ? "Ask about this barcode again"
                        : "Leave this barcode out of the run"
                )
            }
        }
    }

    // MARK: - Catalog #

    /// A chip per number in effect — picked, or printed by the folder and
    /// carried by a release found — with each provider's answer, then each
    /// struck-out number, struck through. Clicking one strikes it out or puts
    /// it back.
    @ViewBuilder
    private var catalogChips: some View {
        let label = String(localized: "Catalog #")
        switch run.catalog {
        case .noneFound:
            MacIdentifierChip(label: label)
                .help("None found")
        case .numbers(let rows, _, _):
            ForEach(rows, id: \.value) { row in
                MacIdentifierChip(
                    label: label,
                    value: row.value,
                    style: row.excluded ? .struck : .filled,
                    onPress: { onToggleCatalogAgreement(row.value) }
                ) {
                    // A struck-out number is asked of nobody.
                    if !row.excluded {
                        capsules(row.cells)
                    }
                }
                .help(
                    row.excluded
                        ? "Put this catalog number back in the run"
                        : "Take this catalog number out of the run"
                )
            }
        }
    }

    // MARK: - ISRC

    /// One chip with MusicBrainz's capsule, the only catalog asked about
    /// ISRCs: every code the files' tags carry, asked in one search. It shows
    /// the first code and, behind +N, every code.
    @ViewBuilder
    private var isrcChip: some View {
        let label = SignalBadgeStyle.label(for: SignalBadgeStyle.Signal.isrc)
        switch run.isrc {
        case .reading:
            MacIdentifierChip(label: label) { ChipWait(waitingOn: run.isrc) }
        case .absent:
            MacIdentifierChip(label: label)
                .help("No ISRC in the files’ tags")
        // Only Settings can switch on the catalog that answers ISRCs.
        case .read(let shown, let moreCount, let isrcs, .notAsked(reason: _)):
            MacIdentifierChip(
                label: label,
                values: MacIdentifierChipValues(
                    shown: shown,
                    moreCount: moreCount,
                    all: isrcs
                )
            )
        case .read(let shown, let moreCount, let isrcs, let lookup):
            MacIdentifierChip(
                label: label,
                values: MacIdentifierChipValues(
                    shown: shown,
                    moreCount: moreCount,
                    all: isrcs
                )
            ) {
                ProviderCapsule(
                    source: .musicBrainz,
                    lookup: lookup,
                    onRetry: onRetryFailed
                )
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

    /// The catalog numbers the folder offers that no release confirms: behind
    /// a "+N more" button with their count, or, once it is pressed, each as
    /// its own chip that puts the number into the run, then "Show fewer".
    @ViewBuilder
    private var catalogCandidates: some View {
        let fold = run.catalogCandidateFold
        ForEach(fold.shown(expanded: listsCatalogCandidates), id: \.self) {
            value in
            CatalogCandidateChip(
                value: value,
                onActivate: { onToggleLookup(.chooseCatalog(number: value)) }
            )
        }
        if let title = fold.buttonTitle(expanded: listsCatalogCandidates) {
            CatalogCandidatesDisclosure(
                title: title,
                isExpanded: $listsCatalogCandidates
            )
        }
    }
}

/// The catalog numbers nothing confirmed, as core hands them over, and what
/// the band draws of them.
struct CatalogCandidateFold {
    let values: [String]
    /// How many numbers core counts, which the button shows.
    let count: UInt32

    /// The numbers drawn as chips: none while folded.
    func shown(expanded: Bool) -> [String] {
        expanded ? values : []
    }

    /// The button's words, with the count while folded; `nil` with no number
    /// to fold.
    func buttonTitle(expanded: Bool) -> String? {
        guard count != 0 else { return nil }
        return expanded
            ? String(localized: "Show fewer")
            : String(localized: "+\(Int(count)) more")
    }
}

extension BridgeIdentifyRun {
    /// The catalog numbers the folder offers that are not in effect, as core
    /// lists them, with their count.
    var catalogCandidateFold: CatalogCandidateFold {
        guard case .numbers(_, let candidates, let count) = catalog else {
            return CatalogCandidateFold(values: [], count: 0)
        }
        return CatalogCandidateFold(
            values: candidates.map(\.value),
            count: count
        )
    }
}

extension MacIdentifierBand {
    // MARK: - Title

    /// The words the run searches by when the identifiers name nothing, with
    /// each provider's answer; editing them searches again.
    @ViewBuilder
    private var titleChip: some View {
        switch run.search {
        // A run whose identifiers answered, or that has no title to search
        // by, searched by no title, so the band draws no title search.
        case .notNeeded:
            EmptyView()
        // Not editable until the run reaches the search.
        case .waiting(let album, let artist):
            TitleSearchChip(
                album: album,
                artist: artist,
                isWaiting: true,
                onCommit: onEditTitleSearch
            ) {
                ChipWait(waitingOn: run.search)
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

/// The words the run searches by, drawn as one chip, "Title search", with the
/// artist and the title after it. Pressing the words opens an artist field and
/// a title field; leaving a changed field commits both.
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
    /// Whether the words are open as fields. A search with no words has
    /// nothing to draw as text, so it is always open.
    @State
    private var isEditing = false

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

    private var showsFields: Bool {
        isEditing || (albumText.isEmpty && artistText.isEmpty)
    }

    var body: some View {
        Group {
            if showsFields {
                MacIdentifierChip(label: String(localized: "Artist")) {
                    field("Artist", text: $artistText, field: .artist)
                    IdentifierLabel(text: String(localized: "Title"))
                    field("Title", text: $albumText, field: .album)
                    trailing
                }
            }
            else {
                MacIdentifierChip(label: String(localized: "Title search")) {
                    Button {
                        isEditing = true
                        focused = .artist
                    } label: {
                        Text(
                            verbatim: FactList.joined([artistText, albumText])
                        )
                        .themeText(.mono)
                        .foregroundStyle(Theme.textSecondary)
                        .cutLine(.prose)
                    }
                    .buttonStyle(.accentPlain)
                    .disabled(isWaiting)
                    trailing
                }
            }
        }
        // A new run's words replace what was typed.
        .onChange(of: album) { _, now in albumText = now }
        .onChange(of: artist) { _, now in artistText = now }
        // Leaving a field searches, including moving to the other one; leaving
        // both closes the fields.
        .onChange(of: focused) { was, now in
            if was != nil, was != now { commit() }
            if now == nil { isEditing = false }
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
            .foregroundStyle(Theme.textSecondary)
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
