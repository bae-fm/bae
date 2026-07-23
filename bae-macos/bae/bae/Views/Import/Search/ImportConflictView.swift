import BaeKit
import SwiftUI

/// The per-signal conflict surface, shown when both identity signals returned
/// releases but they don't agree on a single group (or the intersection was
/// empty). One section per signal that produced results, stacked vertically; the
/// user can pick a row directly or exclude a signal (via its section "Ignore"
/// link or the toolbar toggle) to re-derive without it. Replaces the standard
/// banner + results layout in `ImportSearchPane` while active.
struct ImportConflictView: View {
    let results: ConflictResults
    let state: ImportSearchState
    let onToggle: (BridgeExcludedSignal) -> Void
    let onRerun: () -> Void
    let onSearchManually: () -> Void
    /// `nil` suppresses the "Skip identifying" pill — a CD carries no local data
    /// to seed an Unknown import until it's ripped.
    let onAddAsUnknown: (() -> Void)?
    let onSelect: (BridgeMetadataResult) -> Void

    /// The per-signal results that disagree, destructured from the `.conflict`
    /// identify state: the disc-id and barcode releases with their library
    /// statuses, the source the disc-id lookup consulted, and the matched
    /// barcode value (for the section subtitles).
    struct ConflictResults {
        let discidResults: [BridgeMetadataResult]
        let discidLibraryStatuses: [String: BridgeLibraryStatus]
        let barcodeResults: [BridgeMetadataResult]
        let barcodeLibraryStatuses: [String: BridgeLibraryStatus]
        let matchedBarcode: String?
    }

    /// The signals toolbar, shown once core has emitted a transition.
    @ViewBuilder
    private var toolbar: some View {
        if !state.signalsToolbar.signals.isEmpty {
            SignalsToolbarView(
                toolbar: state.signalsToolbar,
                onToggle: onToggle,
                onRerun: onRerun,
                onSearchManually: onSearchManually,
                onAddAsUnknown: onAddAsUnknown,
            )
        }
    }

    var body: some View {
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
                    if !results.discidResults.isEmpty {
                        conflictSection(
                            signal: .disc,
                            title: "DiscID",
                            subtitle: discidSectionSubtitle(
                                count: results.discidResults.count
                            ),
                            results: results.discidResults,
                            libraryStatuses: results.discidLibraryStatuses,
                        )
                    }
                    if !results.barcodeResults.isEmpty {
                        conflictSection(
                            signal: .barcode,
                            title: "Barcode",
                            subtitle: barcodeSectionSubtitle(
                                count: results.barcodeResults.count,
                                matchedBarcode: results.matchedBarcode
                            ),
                            results: results.barcodeResults,
                            libraryStatuses: results.barcodeLibraryStatuses,
                        )
                    }
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 16)
            }
        }
    }

    /// "Signals disagree on identity" banner — warm-amber tint, two-line copy
    /// that names the choice the user has to make.
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
            DiscIdInfoTip()
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .background(Theme.accent.opacity(0.12))
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
        }
    }

    /// One per-signal section: uppercase tracked title + subtitle on a divider
    /// line, an "Ignore" link aligned right, and pressing-shaped rows below.
    @ViewBuilder
    private func conflictSection(
        signal: BridgeExcludedSignal,
        title: LocalizedStringKey,
        subtitle: AttributedString,
        results: [BridgeMetadataResult],
        libraryStatuses: [String: BridgeLibraryStatus],
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
                    onToggle(signal)
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

    /// Disc-ID section subtitle. Disc-ID lookup consults MusicBrainz and nothing
    /// else (`bae_core::identify::discid`), so the database names itself here. The
    /// format string keeps its placeholder — 31 translations interpolate it, and
    /// a brand name is not translated anyway.
    private func discidSectionSubtitle(count: Int) -> AttributedString {
        AttributedString(
            String(localized: "matched \(count) releases on \("MusicBrainz")")
        )
    }

    /// Barcode section subtitle — surface the matched value (monospaced inline)
    /// so the user can correlate against the artwork that produced it. Falls back
    /// to a value-less label when the matched barcode wasn't preserved.
    private func barcodeSectionSubtitle(
        count: Int,
        matchedBarcode: String?
    ) -> AttributedString {
        var subtitle = AttributedString(
            String(localized: "matched \(count) releases")
        )
        if let barcode = matchedBarcode, !barcode.isEmpty {
            subtitle += AttributedString(" · ")
            var mono = AttributedString(barcode)
            mono.font = .system(.caption, design: .monospaced)
            subtitle += mono
        }
        return subtitle
    }

    private func ignoreButtonLabel(signal: BridgeExcludedSignal) -> String {
        switch signal {
        case .disc: String(localized: "Ignore DiscID")
        case .barcode: String(localized: "Ignore Barcode")
        case .catalog: String(localized: "Ignore Catalog")
        }
    }
}

#if DEBUG
    #Preview("Conflict surface") {
        ImportConflictView(
            results: ImportConflictView.ConflictResults(
                discidResults: PreviewData.conflictDiscidResults,
                discidLibraryStatuses: [:],
                barcodeResults: PreviewData.conflictBarcodeResults,
                barcodeLibraryStatuses: [:],
                matchedBarcode: "5051961234567"
            ),
            state: PreviewData.searchStateConflict,
            onToggle: { _ in },
            onRerun: {},
            onSearchManually: {},
            onAddAsUnknown: {},
            onSelect: { _ in },
        )
        .frame(width: 1212, height: 982)
        .importPreviewEnvironment()
    }
#endif
