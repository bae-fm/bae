import BaeKit
import SwiftUI

/// The run as one wrapping band of chips: what identification has to go on,
/// each identifier with where it was read and every provider's answer about
/// it. The three signals come in order — Disc ID, Barcode, Catalog # — then
/// the catalog numbers that rank the answers rather than drive a lookup, then
/// the ones waiting to be looked up.
///
/// Every provider answers on its own, so a person watches the run rather than
/// waiting for it, and a provider that failed offers its own Retry while the
/// others carry on.
struct IdentifierBand: View {
    let run: BridgeIdentifyRun
    /// The numbers one of the offered releases carries, which rank the list
    /// rather than drive a lookup. Empty while the run is still going: which
    /// numbers these are follows from the releases it settles on.
    let catalogAgreements: [BridgeCatalogAgreement]
    /// Turn one identifier in the band over: a disc ID or a barcode the run
    /// asks about is left out and one left out is asked about again, a waiting
    /// catalog number starts being looked up and a running one stops.
    let onToggleLookup: (LookupToggle) -> Void
    /// Count a catalog number the folder states, or stop counting it. Nothing
    /// is looked up either way.
    let onToggleCatalogAgreement: (String) -> Void
    /// Re-ask only the lookups that failed.
    let onRetryFailed: () -> Void

    var body: some View {
        FlowLayout(spacing: 6) {
            discIdChip
            barcodeChips
            catalogChips
            ForEach(catalogAgreements, id: \.value) { agreement in
                CatalogAgreementChip(
                    agreement: agreement,
                    onToggle: { onToggleCatalogAgreement(agreement.value) }
                )
            }
            ForEach(catalogCandidates, id: \.value) { candidate in
                CatalogCandidateChip(
                    candidate: candidate,
                    onActivate: { onToggleLookup(.catalog(candidate.value)) }
                )
            }
            if isScanning {
                ScanningChip()
            }
        }
        .padding(.vertical, 9)
        .padding(.horizontal, 14)
    }

    // MARK: - Disc ID

    /// One chip, whatever the step has reached. The disc-ID endpoint is
    /// MusicBrainz's alone, so the chip carries that one provider's capsule.
    /// A disc ID that was read is a button: clicking it leaves it out of the
    /// run, and clicking it again asks about it.
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
        case .readFailed(let failure):
            IdentifierChip(label: label) { IdentifierWarning() }
                .help(
                    String(
                        localized:
                            "Couldn't read the disc layout: \(failure.briefLine)"
                    )
                )
        case .read(let discId, let source, let lookup):
            discIdButton(
                label: label,
                discId: discId,
                source: source,
                lookup: lookup
            )
        // The person took the disc ID out, so the chip reads as off and the
        // way back is the chip itself.
        case .leftOut(let discId, let source):
            discIdButton(
                label: label,
                discId: discId,
                source: source,
                lookup: nil
            )
        // The one source that answers disc IDs is not being asked, so the
        // value stands with a dash where a count would be — nothing looked,
        // which is not the same as looking and finding none. There is nothing
        // here for a person to switch: which sources a run asks is a Settings
        // switch, not this chip.
        case .readNotAsked(let discId, let source):
            IdentifierChip(
                label: label,
                tags: .discIdFile(source),
                value: discId
            ) {
                IdentifierDash()
            }
        }
    }

    /// The disc ID as a switch. `lookup` is MusicBrainz's answer about it, and
    /// is `nil` for a disc ID the person left out: nobody was asked, so there
    /// is no answer to carry and the chip reads as off.
    private func discIdButton(
        label: String,
        discId: String,
        source: BridgeDiscIdFile?,
        lookup: BridgeLookupState?
    ) -> some View {
        Button {
            onToggleLookup(.discId)
        } label: {
            IdentifierChip(
                label: label,
                tags: .discIdFile(source),
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

    /// A chip per code the folder carries. Each is a button: clicking a code
    /// the run asks about leaves it out, and clicking one left out asks about
    /// it again.
    @ViewBuilder
    private var barcodeChips: some View {
        let label = SignalBadgeStyle.label(for: BridgeSignalKind.barcode)
        switch run.barcode {
        case .absent:
            IdentifierChip(label: label) { IdentifierDash() }
                .help("No barcode source")
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
                        tags: .sources(row.sources),
                        value: row.value,
                        style: row.excluded ? .outlined : .filled
                    ) {
                        // A code nobody was asked about has no answer to
                        // carry, so the chip is the value alone.
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

    /// A chip per number the run is looking up. Each is the chip it was
    /// activated from: clicking it takes the number back out of the run and
    /// drops its results from the list.
    @ViewBuilder
    private var catalogChips: some View {
        let label = String(localized: "Catalog #")
        switch run.catalog {
        case .noneFound:
            IdentifierChip(label: label) { IdentifierDash() }
                .help("None found")
        case .numbers(_, let rows, _):
            ForEach(rows, id: \.value) { row in
                Button {
                    onToggleLookup(.catalog(row.value))
                } label: {
                    IdentifierChip(
                        label: label,
                        tags: .sources(row.sources),
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

    /// The numbers extraction found that the run is not looking up and no
    /// release came back carrying. Each waits to be activated: one number can
    /// name thirty releases, so none of them runs on its own.
    private var catalogCandidates: [BridgeCatalogCandidate] {
        guard case .numbers(_, _, let candidates) = run.catalog else {
            return []
        }
        return candidates
    }

    /// Whether the artwork is still being read, so more chips may join the
    /// band. Both steps read the same scan, so one spinner answers for both.
    private var isScanning: Bool {
        if case .rows(scanning: true, rows: _) = run.barcode { return true }
        if case .numbers(scanning: true, rows: _, candidates: _) = run.catalog {
            return true
        }
        return false
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
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    /// One source asked: one capsule per chip, and a disc ID with a dash where
    /// a count would be because the source that answers disc IDs is not being
    /// asked.
    #Preview("Run on one source") {
        IdentifierBand(
            run: PreviewData.identifyRunOneSource,
            catalogAgreements: [],
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
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
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    /// The same run as "A provider failed", with its catalog number taken back
    /// out: the number's chip loses its capsules and joins the outlined ones,
    /// beside the numbers the answers themselves carry.
    #Preview("A catalog number waiting to be used") {
        IdentifierBand(
            run: PreviewData.identifyRunCatalogWaiting,
            catalogAgreements: PreviewData.catalogAgreements,
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    /// The off chips: a disc ID the person took out of the run, and one of two
    /// barcodes left out beside the one still being looked up — over the same
    /// two barcodes both asked about, for the difference.
    #Preview("Identifiers left out of the run") {
        VStack(alignment: .leading, spacing: 0) {
            IdentifierBand(
                run: PreviewData.identifyRunDiscIdLeftOut,
                catalogAgreements: [],
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
            )
            IdentifierBand(
                run: PreviewData.identifyRunBothBarcodesAsked,
                catalogAgreements: [],
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
            )
            IdentifierBand(
                run: PreviewData.identifyRunBarcodeLeftOut,
                catalogAgreements: [],
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
            )
        }
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
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }
#endif
