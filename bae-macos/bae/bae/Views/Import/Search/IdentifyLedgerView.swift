import BaeKit
import SwiftUI

/// The run as a ledger: the three signals in order — Disc ID, Barcode,
/// Catalog # — each with one row per value extraction found, where it was
/// found beside the value, and one cell per provider asked about it. Every
/// cell settles on its own, so a person watches the run rather than waiting
/// for it, and a provider that failed offers its own Retry while the others
/// carry on.
///
/// The disc ID sits above the provider table: MusicBrainz alone answers it,
/// so its one count follows the value inline. The table's columns stay
/// drawn with no rows in them, so a catalog number activated later has
/// somewhere to land.
struct IdentifyLedgerView: View {
    let run: BridgeIdentifyRun
    /// The numbers one of the offered releases carries, which rank the list
    /// rather than drive a lookup. Empty while the run is still going: which
    /// numbers these are follows from the releases it settles on.
    let catalogAgreements: [BridgeCatalogAgreement]
    /// Where each of the candidate's files is on disk, by the path a source
    /// names it by — what an artwork chip crops its thumbnail out of.
    let filePaths: [String: String]
    /// Take a catalog number in or out of the run: a tile becomes a row, a
    /// row becomes a tile again.
    let onToggleCatalog: (String) -> Void
    /// Count a catalog number the folder states, or stop counting it. Nothing
    /// is looked up either way.
    let onToggleCatalogAgreement: (String) -> Void
    /// Re-ask only the lookups that failed.
    let onRetryFailed: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            discIdGroup
            providerTable
            catalogTiles
        }
        .padding(.bottom, 10)
    }

    // MARK: - Disc ID

    @ViewBuilder
    private var discIdGroup: some View {
        let label = SignalBadgeStyle.label(for: BridgeSignalKind.discId)
        switch run.discId {
        case .reading:
            LedgerGroupLabel(text: label, working: true)
                .help("Reading…")
        case .absent:
            LedgerGroupLabel(text: label, nothing: true)
                .help("No LOG or CUE in the folder")
        case .readFailed(let failure):
            LedgerGroupLabel(text: label, failure: failure)
                .help(
                    String(
                        localized:
                            "Couldn't read the disc layout: \(failure.briefLine)"
                    )
                )
        case .read(let discId, let source, let lookup):
            LedgerGroupLabel(text: label)
            LedgerRowBand {
                HStack(spacing: 7) {
                    if let source {
                        DiscIdFileChip(source: source)
                    }
                    LedgerValueText(value: discId)
                }
                LookupCellView(lookup: lookup, onRetry: onRetryFailed)
                    .fixedSize()
                    .padding(.leading, 10)
                Spacer(minLength: 0)
            }
        // The one source that answers disc IDs is not being asked, so the
        // value stands with a dash where a count would be — nothing looked,
        // which is not the same as looking and finding none.
        case .readNotAsked(let discId, let source):
            LedgerGroupLabel(text: label)
            LedgerRowBand {
                HStack(spacing: 7) {
                    if let source {
                        DiscIdFileChip(source: source)
                    }
                    LedgerValueText(value: discId)
                }
                Text(verbatim: "\u{2013}")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                    .padding(.leading, 10)
                Spacer(minLength: 0)
            }
        }
    }

    // MARK: - Provider table

    /// The barcode and catalog rows under the provider columns, with the
    /// column rails drawn behind them.
    private var providerTable: some View {
        VStack(alignment: .leading, spacing: 0) {
            columnHeaders
            barcodeGroup
            catalogGroup
        }
        .background(alignment: .trailing) {
            LedgerColumnRails(columns: run.providers.count)
                .padding(.vertical, 2)
        }
    }

    /// The provider names, once, above the first row. Text, no icons.
    private var columnHeaders: some View {
        HStack(spacing: 0) {
            Spacer(minLength: 0)
            ForEach(run.providers, id: \.self) { source in
                Text(bridgeMetadataSourceName(source: source))
                    .font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: LedgerMetrics.cellWidth)
            }
        }
        .padding(.horizontal, LedgerMetrics.sideInset)
        .frame(height: 24)
    }

    // MARK: - Barcode

    @ViewBuilder
    private var barcodeGroup: some View {
        let label = SignalBadgeStyle.label(for: BridgeSignalKind.barcode)
        switch run.barcode {
        case .absent:
            LedgerGroupLabel(text: label, nothing: true)
                .help("No barcode source")
        case .noCodes:
            LedgerGroupLabel(text: label, nothing: true)
                .help("No barcode on the artwork")
        case .scanFailed(let failure):
            LedgerGroupLabel(text: label, failure: failure)
                .help(
                    String(
                        localized:
                            "Couldn't read the barcodes: \(failure.briefLine)"
                    )
                )
        case .rows(let scanning, let rows):
            LedgerGroupLabel(text: label, working: scanning)
            ForEach(rows, id: \.value) { row in
                LedgerValueRow(
                    row: row,
                    filePaths: filePaths,
                    onRetry: onRetryFailed
                )
            }
        }
    }

    // MARK: - Catalog #

    @ViewBuilder
    private var catalogGroup: some View {
        let label = String(localized: "Catalog #")
        switch run.catalog {
        case .noneFound:
            LedgerGroupLabel(text: label, nothing: true)
                .help("None found")
        case .numbers(let scanning, let rows, _):
            LedgerGroupLabel(text: label, working: scanning)
            // An active row is the tile it was promoted from: clicking it
            // demotes it and drops its results from the list.
            ForEach(rows, id: \.value) { row in
                Button {
                    onToggleCatalog(row.value)
                } label: {
                    LedgerValueRow(
                        row: row,
                        filePaths: filePaths,
                        onRetry: onRetryFailed
                    )
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Take this catalog number out of the run")
            }
        }
    }

    /// One row of chips under the table: first the numbers one of the offered
    /// releases carries, which rank the list, then the numbers nothing came
    /// back carrying. The second kind never runs on its own — one number can
    /// name thirty releases, so each waits to be activated.
    @ViewBuilder
    private var catalogTiles: some View {
        if !catalogAgreements.isEmpty || !catalogCandidates.isEmpty {
            FlowLayout(spacing: 6) {
                ForEach(catalogAgreements, id: \.value) { agreement in
                    CatalogAgreementChip(
                        agreement: agreement,
                        onToggle: {
                            onToggleCatalogAgreement(agreement.value)
                        }
                    )
                }
                ForEach(catalogCandidates, id: \.value) { candidate in
                    CatalogCandidateTile(
                        candidate: candidate,
                        filePaths: filePaths,
                        onActivate: { onToggleCatalog(candidate.value) }
                    )
                }
            }
            .padding(.top, 3)
            .padding(.bottom, 4)
            .padding(.leading, LedgerMetrics.rowInset)
            .padding(.trailing, LedgerMetrics.sideInset)
        }
    }

    /// The numbers extraction found that the run is not looking up and no
    /// release came back carrying.
    private var catalogCandidates: [BridgeCatalogCandidate] {
        guard case .numbers(_, _, let candidates) = run.catalog else {
            return []
        }
        return candidates
    }
}

/// The ledger's fixed measurements: one cell per provider column, and the
/// insets rows and labels share so the columns line up down the table.
enum LedgerMetrics {
    static let cellWidth: CGFloat = 92
    static let sideInset: CGFloat = 14
    static let rowInset: CGFloat = 28
}

/// The vertical hairlines at each provider column's left edge, so the table
/// stays anchored with no rows in it.
struct LedgerColumnRails: View {
    let columns: Int

    var body: some View {
        HStack(spacing: 0) {
            ForEach(0..<columns, id: \.self) { _ in
                Rectangle()
                    .fill(Color.primary.opacity(0.05))
                    .frame(width: 1)
                Color.clear
                    .frame(width: LedgerMetrics.cellWidth - 1)
            }
            Color.clear
                .frame(width: LedgerMetrics.sideInset)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Run in flight") {
        IdentifyLedgerView(
            run: PreviewData.identifyRunInFlight,
            catalogAgreements: [],
            filePaths: [:],
            onToggleCatalog: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    /// One source asked: one column, one rail, and a disc ID with a dash
    /// where a count would be because the source that answers disc IDs is not
    /// being asked.
    #Preview("Run on one source") {
        IdentifyLedgerView(
            run: PreviewData.identifyRunOneSource,
            catalogAgreements: [],
            filePaths: [:],
            onToggleCatalog: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    #Preview("Run starting") {
        IdentifyLedgerView(
            run: PreviewData.identifyRunStarting,
            catalogAgreements: [],
            filePaths: [:],
            onToggleCatalog: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    #Preview("A provider failed") {
        IdentifyLedgerView(
            run: PreviewData.identifyRunProviderFailed,
            catalogAgreements: [],
            filePaths: [:],
            onToggleCatalog: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    /// The same run as "A provider failed", with its catalog number taken back
    /// out: the number leaves the table and rejoins the chips under it, beside
    /// the numbers the answers themselves carry.
    #Preview("A catalog number waiting to be used") {
        IdentifyLedgerView(
            run: PreviewData.identifyRunCatalogWaiting,
            catalogAgreements: PreviewData.catalogAgreements,
            filePaths: [:],
            onToggleCatalog: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }

    #Preview("Nothing found") {
        IdentifyLedgerView(
            run: PreviewData.identifyRunNothingFound,
            catalogAgreements: [],
            filePaths: [:],
            onToggleCatalog: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }
#endif
