import AppKit
import BaeKit
import SwiftUI

/// One pressing row beneath a release-group card: the year, the label, the
/// catalogue number, where and in what format it was pressed, which signals
/// named it, and every source that lists it.
///
/// The row is picked whole: it commits the pressing's lead release and carries
/// every other source's record of the same pressing along with it. That opens
/// the docked confirm pane, where the Exact / Metadata-only choice is made.
///
/// A row already in the library still opens the pane — it surfaces the
/// "already imported" banner and leaves Import disabled there — so the row
/// stays clickable; it only dims to signal the dupe.
struct ImportSearchResultRow: View {
    let pressing: Pressing
    let isImporting: Bool
    let libraryStatus: BridgeLibraryStatus?
    /// Which signals produced or confirmed this pressing, for the badge row.
    /// `nil` for typed-search results, which no signal produced.
    var provenance: BridgeResultProvenance?
    let isSelected: Bool
    /// Whether this row's pick is being read right now — the row itself
    /// carries the spinner, so the list stays put while the release loads.
    var isLoading: Bool = false
    let onSelect: (Pressing) -> Void

    private var isInLibrary: Bool {
        libraryStatus?.releaseInLibrary == true
    }

    var body: some View {
        ZStack {
            Button {
                onSelect(pressing)
            } label: {
                Rectangle()
                    .fill(.clear)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(isImporting)

            HStack(spacing: 8) {
                facts
                    .opacity(isInLibrary ? 0.55 : 1)
                    .allowsHitTesting(false)
                signalBadges
                    .allowsHitTesting(false)
                Spacer(minLength: 8)
                libraryMarker
                    .allowsHitTesting(false)
                sourceTags
                    .allowsHitTesting(false)
                chevron
                    .allowsHitTesting(false)
            }
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 10)
        .background(rowBackground)
    }

    private var rowBackground: some View {
        RoundedRectangle(cornerRadius: 7)
            .fill(isSelected ? Theme.accent.opacity(0.12) : .clear)
            .overlay(
                RoundedRectangle(cornerRadius: 7)
                    .strokeBorder(
                        isSelected ? Theme.accent.opacity(0.4) : .clear,
                        lineWidth: 1
                    )
            )
    }

    // MARK: - What the pressing is

    private var facts: some View {
        HStack(spacing: 8) {
            if let year = pressing.lead.year {
                Text(String(year))
                    .font(.system(size: 13, weight: .semibold))
                    .monospacedDigit()
            }
            else {
                Text("Year unknown")
                    .font(.system(size: 12))
                    .foregroundStyle(.tertiary)
            }
            if let label = pressing.lead.label {
                Text(label)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            if let catalogNumber = pressing.lead.catalogNumber {
                Text(catalogNumber)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(
                        Theme.hover,
                        in: RoundedRectangle(cornerRadius: 4)
                    )
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            if !pressed.isEmpty {
                Text(pressed)
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
    }

    /// Where it was pressed and in what form — "US · CD" — joined only where
    /// the source states both.
    private var pressed: String {
        [pressing.lead.country, pressing.lead.format]
            .compactMap { $0 }
            .joined(separator: " \u{00b7} ")
    }

    // MARK: - Signal badges

    /// Which signals produced or confirmed this row. All three chips stay in
    /// the tree (opacity-toggled) so row height is stable across the list.
    @ViewBuilder
    private var signalBadges: some View {
        if let provenance {
            HStack(spacing: 4) {
                signalBadge(.discId, on: provenance.byDiscId)
                signalBadge(.barcode, on: provenance.byBarcode)
                signalBadge(.catalog, on: provenance.byCatalog)
            }
        }
    }

    /// Signal badges use the accent as an informational tint.
    private func signalBadge(_ kind: BridgeSignalKind, on: Bool) -> some View {
        Text(SignalBadgeStyle.label(for: kind))
            .font(.system(size: 10.5, weight: .semibold))
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(Color.accentColor.opacity(0.15), in: Capsule())
            .foregroundStyle(Color.accentColor)
            .opacity(on ? 1 : 0)
            .accessibilityHidden(!on)
    }

    // MARK: - Trailing

    /// Every source listing this pressing, named. A label, not a choice: the
    /// row is one pressing however many sources carry it, and picking it
    /// claims all of them.
    private var sourceTags: some View {
        HStack(spacing: 4) {
            ForEach(pressing.releases) { release in
                if release.releaseId != pressing.lead.releaseId {
                    Text(verbatim: "\u{00b7}")
                        .font(.system(size: 11))
                        .foregroundStyle(.quaternary)
                }
                Text(bridgeMetadataSourceName(source: release.source))
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }
        }
    }

    /// "In library" tag for an already-imported pressing. Kept in the tree
    /// (opacity-toggled) so selecting a row doesn't reflow the column.
    private var libraryMarker: some View {
        HStack(spacing: 4) {
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green)
            Text("In library")
        }
        .font(.system(size: 11))
        .foregroundStyle(.tertiary)
        .opacity(isInLibrary ? 1 : 0)
        .accessibilityHidden(!isInLibrary)
    }

    /// Both trailing states stay in the tree (opacity-swapped): rows render
    /// in a repeated list, and conditional inclusion would re-measure every
    /// sibling when a pick starts loading.
    private var chevron: some View {
        ZStack {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.7)
                .opacity(isLoading ? 1 : 0)
            Image(systemName: "chevron.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(
                    isSelected ? Theme.accent : Color.primary.opacity(0.3)
                )
                .opacity(isLoading ? 0 : 1)
        }
        .frame(width: 12)
    }
}

#if DEBUG
    // MARK: - Preview

    #Preview("Pressing rows") {
        VStack(spacing: 1) {
            ImportSearchResultRow(
                pressing: PreviewData.searchGroupExact.pressings[0],
                isImporting: false,
                libraryStatus: nil,
                provenance: BridgeResultProvenance(
                    byDiscId: true,
                    byBarcode: false,
                    byCatalog: true
                ),
                isSelected: true,
                onSelect: { _ in },
            )
            ImportSearchResultRow(
                pressing: PreviewData.searchGroupsManual[1].pressings[0],
                isImporting: false,
                libraryStatus: nil,
                provenance: nil,
                isSelected: false,
                isLoading: true,
                onSelect: { _ in },
            )
            ImportSearchResultRow(
                pressing: PreviewData.searchGroupExact.pressings[1],
                isImporting: false,
                libraryStatus: BridgeLibraryStatus(
                    releaseId: "rel-456",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "Album Title",
                    albumId: "album-1",
                ),
                provenance: nil,
                isSelected: false,
                onSelect: { _ in },
            )
        }
        .padding()
        .frame(width: 620)
        .importPreviewEnvironment()
    }
#endif
