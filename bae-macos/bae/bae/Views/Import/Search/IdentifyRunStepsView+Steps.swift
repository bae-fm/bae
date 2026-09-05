import BaeKit
import SwiftUI

// The rows of each step, and the provider rows nested under them.
extension IdentifyRunStepsView {
    // MARK: - Disc ID

    @ViewBuilder
    var discIdRows: some View {
        let label = SignalBadgeStyle.label(for: .discId)
        switch run.discId {
        case .reading:
            StepRow(glyph: .working, label: label) {
                outcome(String(localized: "Reading\u{2026}"))
            }
        case .absent:
            StepRow(glyph: .none, label: label, dimmed: true) {
                outcome(
                    String(localized: "No LOG or CUE in the folder"),
                    dimmed: true
                )
            }
        case .readFailed(let failure):
            StepRow(glyph: .failed, label: label) {
                warning(
                    String(
                        localized:
                            "Couldn't read the disc layout: \(failure.briefLine)"
                    )
                )
            }
            .help(failure.badgeLine)
        case .read(let discId, let sourceFile, let lookup):
            StepRow(glyph: .done, label: label, value: discId) {
                if let sourceFile {
                    outcome(String(localized: "from \(fileName(sourceFile))"))
                }
            }
            // The disc-ID endpoint is MusicBrainz's alone.
            lookupRow(source: .musicBrainz, state: lookup)
        }
    }

    // MARK: - Artwork

    @ViewBuilder
    var artworkRow: some View {
        switch run.artwork {
        case .absent:
            StepRow(
                glyph: .none,
                label: String(localized: "Artwork"),
                dimmed: true
            ) {
                outcome(String(localized: "No artwork to read"), dimmed: true)
            }
        case .reading(
            let current,
            let position,
            let total,
            let barcodes,
            let catalogs
        ):
            StepRow(
                glyph: .working,
                label: String(localized: "Reading artwork"),
                value: current.map(fileName),
                position: String(
                    localized: "\(Int(position)) of \(Int(total))"
                )
            ) {
                outcome(foundSummary(barcodes: barcodes, catalogs: catalogs))
            }
        case .read(let images, let barcodes, let catalogs):
            StepRow(
                glyph: .done,
                label: String(localized: "Artwork"),
                position: String(localized: "\(Int(images)) images")
            ) {
                outcome(foundSummary(barcodes: barcodes, catalogs: catalogs))
            }
        case .failed(let failure, let read, let total):
            StepRow(
                glyph: .failed,
                label: String(localized: "Artwork"),
                position: String(localized: "\(Int(read)) of \(Int(total))")
            ) {
                warning(
                    String(
                        localized:
                            "Couldn't read the artwork: \(failure.briefLine)"
                    )
                )
            }
            .help(failure.badgeLine)
        }
    }

    /// "1 barcode · 3 catalog numbers" — what the images read so far turned up.
    func foundSummary(barcodes: UInt32, catalogs: UInt32) -> String {
        [
            String(localized: "\(Int(barcodes)) barcodes"),
            String(localized: "\(Int(catalogs)) catalog numbers"),
        ]
        .joined(separator: " \u{00b7} ")
    }

    // MARK: - Barcode

    @ViewBuilder
    var barcodeRows: some View {
        let label = SignalBadgeStyle.label(for: .barcode)
        switch run.barcode {
        case .awaitingArtwork:
            StepRow(glyph: .waiting, label: label, dimmed: true) {
                outcome(String(localized: "After artwork"), dimmed: true)
            }
            ForEach(run.providers, id: \.self) { source in
                StepRow(
                    glyph: .waiting,
                    label: bridgeMetadataSourceName(source: source),
                    nested: true,
                    dimmed: true
                ) {
                    EmptyView()
                }
            }
        case .absent:
            StepRow(glyph: .none, label: label, dimmed: true) {
                outcome(String(localized: "No barcode source"), dimmed: true)
            }
        case .noCodes:
            StepRow(glyph: .done, label: label) {
                outcome(
                    String(localized: "No barcode on the artwork"),
                    dimmed: true
                )
            }
        case .scanFailed(let failure):
            StepRow(glyph: .failed, label: label) {
                warning(
                    String(
                        localized:
                            "Couldn't read the barcodes: \(failure.briefLine)"
                    )
                )
            }
            .help(failure.badgeLine)
        case .lookups(let codes, let providers):
            let working = providers.contains {
                if case .trying = $0.state {
                    true
                }
                else {
                    false
                }
            }
            StepRow(
                glyph: working ? .working : .done,
                label: label,
                value: codes.joined(separator: " \u{00b7} ")
            ) {
                EmptyView()
            }
            ForEach(providers, id: \.source) { provider in
                barcodeLookupRow(provider, codes: codes)
            }
        }
    }

    /// One provider's walk: the code it is on, when there are several to
    /// walk, and how far it has got.
    @ViewBuilder
    func barcodeLookupRow(
        _ provider: BridgeProviderBarcodeLookup,
        codes: [String]
    ) -> some View {
        let name = bridgeMetadataSourceName(source: provider.source)
        let several = codes.count > 1
        switch provider.state {
        case .trying(let barcode, let position, let total):
            StepRow(
                glyph: .working,
                label: name,
                value: several ? barcode : nil,
                position: several
                    ? String(localized: "\(Int(position)) of \(Int(total))")
                    : nil,
                nested: true
            ) {
                outcome(String(localized: "Looking up\u{2026}"))
            }
        case .matched(let barcode, let count):
            StepRow(
                glyph: .done,
                label: name,
                value: several ? barcode : nil,
                nested: true
            ) {
                CountCapsule(count: Int(count))
            }
        case .exhausted:
            StepRow(glyph: .done, label: name, nested: true) {
                CountCapsule(count: 0)
            }
        case .failed(let failure):
            StepRow(glyph: .failed, label: name, nested: true) {
                failed(failure)
            }
            .help(failure.badgeLine)
        }
    }

    // MARK: - Catalog number

    @ViewBuilder
    var catalogRows: some View {
        let label = String(localized: "Catalog number")
        switch run.catalog {
        case .noneFound:
            StepRow(glyph: .none, label: label, dimmed: true) {
                outcome(String(localized: "None found"), dimmed: true)
            }
        case .unchosen(let available):
            StepRow(glyph: .waiting, label: label, dimmed: true) {
                catalogPicker(
                    title: String(localized: "Pick one of \(Int(available))")
                )
            }
        case .chosen(let value, let lookups):
            let working = lookups.contains { $0.state == .lookingUp }
            StepRow(
                glyph: working ? .working : .done,
                label: label,
                value: value
            ) {
                catalogPicker(title: nil)
            }
            ForEach(lookups, id: \.source) { lookup in
                lookupRow(source: lookup.source, state: lookup.state)
            }
        }
    }

    /// The row's way of telling the run which number to look up. Opens the
    /// numbers in a popover the row keeps its shape under.
    func catalogPicker(title: String?) -> some View {
        Button {
            isPickingCatalog = true
        } label: {
            HStack(spacing: 4) {
                if let title {
                    Text(title)
                }
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
            }
            .font(.system(size: 11.5))
        }
        .buttonStyle(.link)
        .disabled(catalogOptions.isEmpty)
        .help(String(localized: "Pick a catalog number to look up"))
        .popover(isPresented: $isPickingCatalog, arrowEdge: .bottom) {
            CatalogOptionsList(options: catalogOptions) { value in
                isPickingCatalog = false
                onToggleSignal(.catalog(value: value))
            }
            .padding(9)
            .frame(width: 280)
            .popoverEntrance(anchor: .top)
            .background { PopoverBehavior() }
        }
    }

    // MARK: - Provider rows

    /// One provider's part of a single-value lookup — the disc ID's, or the
    /// chosen catalog number's.
    @ViewBuilder
    func lookupRow(
        source: BridgeMetadataSource,
        state: BridgeLookupState
    ) -> some View {
        let name = bridgeMetadataSourceName(source: source)
        switch state {
        case .lookingUp:
            StepRow(glyph: .working, label: name, nested: true) {
                outcome(String(localized: "Looking up\u{2026}"))
            }
        case .found(let count):
            StepRow(glyph: .done, label: name, nested: true) {
                CountCapsule(count: Int(count))
            }
        case .noMatch:
            StepRow(glyph: .done, label: name, nested: true) {
                CountCapsule(count: 0)
            }
        case .failed(let failure):
            StepRow(glyph: .failed, label: name, nested: true) {
                failed(failure)
            }
            .help(failure.badgeLine)
        }
    }

    // MARK: - Outcomes

    func outcome(_ text: String, dimmed: Bool = false) -> some View {
        Text(text)
            .font(.system(size: 11.5))
            .foregroundStyle(
                dimmed ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.secondary)
            )
            .lineLimit(1)
            .truncationMode(.middle)
    }

    func warning(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 11.5))
            .foregroundStyle(.orange)
            .lineLimit(1)
            .truncationMode(.middle)
    }

    /// A provider that failed: why, briefly, and the way to ask it again.
    func failed(_ failure: BridgeLookupFailure) -> some View {
        HStack(spacing: 8) {
            warning(failure.briefLine)
            Button("Retry", action: onRetryFailed)
                .buttonStyle(.link)
                .font(.system(size: 11.5))
        }
    }

    /// The last path component of a candidate-relative path: the file a
    /// person recognises, not the folder it sits in.
    func fileName(_ path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }
}
