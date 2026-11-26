import BaeKit
import SwiftUI

/// The run as one wrapping band of chips — Disc ID, Barcode, Catalog #, ISRC,
/// the title search, then the catalog numbers nothing confirmed folded behind
/// their count — each in the look of what its lookups found.
struct MacIdentifierBand: View {
    let run: BridgeIdentifyRun
    /// Leave an identifier out of the run, or ask about it again.
    let onToggleLookup: (BridgeLookupChoiceEdit) -> Void
    /// Leave a catalog number in effect out of the run, or put a left-out one
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

    /// One chip per Disc ID, the only identifier MusicBrainz alone answers; a
    /// read Disc ID toggles the Disc IDs in and out of the run.
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
        // Only Settings can switch on the catalog that answers Disc IDs.
        if case .notAsked(reason: .noCatalog) = disc.lookup {
            MacIdentifierChip(label: label, value: disc.discId)
        }
        else {
            discIdButton(label: label, disc: disc)
        }
    }

    /// The Disc ID as a toggle: left out, pressing it asks about it again.
    private func discIdButton(label: String, disc: BridgeDiscIdRow)
        -> some View
    {
        MacIdentifierChip(
            label: label,
            value: disc.discId,
            style: MacIdentifierChipStyle(disc.chip.look),
            onPress: { onToggleLookup(.toggleDiscId) }
        ) {
            ChipTrailing(state: disc.chip.trailing, onRetry: onRetryFailed)
        }
        .help(
            disc.chip.look == .off
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
                    style: MacIdentifierChipStyle(row.chip.look),
                    onPress: { onToggleLookup(.toggleBarcode(code: row.value)) }
                ) {
                    ChipTrailing(
                        state: row.chip.trailing,
                        onRetry: onRetryFailed
                    )
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
    /// carried by a release found — then each number left out, an off chip.
    /// Clicking one leaves it out or puts it back.
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
                    style: MacIdentifierChipStyle(row.chip.look),
                    onPress: { onToggleCatalogAgreement(row.value) }
                ) {
                    ChipTrailing(
                        state: row.chip.trailing,
                        onRetry: onRetryFailed
                    )
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

    /// One chip for every code the files' tags carry, asked of MusicBrainz,
    /// the only catalog asked about ISRCs, in one search. It shows the first
    /// code and, behind +N, every code.
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
        case .read(let shown, let moreCount, let isrcs, .notAsked(reason: _), _):
            MacIdentifierChip(
                label: label,
                values: MacIdentifierChipValues(
                    shown: shown,
                    moreCount: moreCount,
                    all: isrcs
                )
            )
        case .read(let shown, let moreCount, let isrcs, _, let chip):
            MacIdentifierChip(
                label: label,
                values: MacIdentifierChipValues(
                    shown: shown,
                    moreCount: moreCount,
                    all: isrcs
                ),
                style: MacIdentifierChipStyle(chip.look)
            ) {
                ChipTrailing(state: chip.trailing, onRetry: onRetryFailed)
            }
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

    /// The words the run searches by when the identifiers name nothing;
    /// editing them searches again.
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
                style: .on,
                isWaiting: true,
                onCommit: onEditTitleSearch
            ) {
                ChipWait(waitingOn: run.search)
            }
        case .searched(let album, let artist, let chip):
            TitleSearchChip(
                album: album,
                artist: artist,
                style: MacIdentifierChipStyle(chip.look),
                onCommit: onEditTitleSearch
            ) {
                ChipTrailing(state: chip.trailing, onRetry: onRetryFailed)
            }
        }
    }
}

/// The words the run searches by, drawn as one chip that reads Artist, the
/// artist's field, Title and the title's field. Leaving a changed field
/// commits both.
struct TitleSearchChip<Trailing: View>: View {
    let album: String
    let artist: String
    /// The chip's look: found where the search found releases.
    let style: MacIdentifierChipStyle
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

    /// The least width of a field with no words in it.
    private static var fieldLeastWidth: CGFloat { 14 }

    private enum Field {
        case album
        case artist
    }

    init(
        album: String,
        artist: String,
        style: MacIdentifierChipStyle = .found,
        isWaiting: Bool = false,
        onCommit: @escaping (_ album: String, _ artist: String) -> Void,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.album = album
        self.artist = artist
        self.style = style
        self.isWaiting = isWaiting
        self.onCommit = onCommit
        self.trailing = trailing()
        _albumText = State(initialValue: album)
        _artistText = State(initialValue: artist)
    }

    var body: some View {
        MacIdentifierChipFrame(style: style) {
            IdentifierLabel(text: String(localized: "Artist"), style: style)
            field(
                "Artist",
                text: $artistText,
                field: .artist,
                overlapsNext: false
            )
            IdentifierLabel(text: String(localized: "Title"), style: style)
            field("Title", text: $albumText, field: .album, overlapsNext: true)
            trailing
                .fixedSize(horizontal: true, vertical: false)
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
        field: Field,
        overlapsNext: Bool
    ) -> some View {
        // The field is as wide as its words, which a hidden copy of them
        // sets, so the typed text never leaves a gap after it.
        Text(verbatim: text.wrappedValue)
            .themeText(.detail)
            .fixedSize()
            .hidden()
            .frame(minWidth: Self.fieldLeastWidth, alignment: .leading)
            .overlay {
                TextField(name, text: text, prompt: Text(verbatim: ""))
                    .textFieldStyle(.plain)
                    .themeText(.detail)
                    .foregroundStyle(style.valueColor)
                    .focused($focused, equals: field)
                    .disabled(isWaiting)
                    .onSubmit { focused = nil }
                    .accessibilityLabel(name)
            }
            .identifierInset(overlapsNext: overlapsNext)
    }

    private func commit() {
        if albumText == album && artistText == artist { return }
        onCommit(albumText, artistText)
    }
}
