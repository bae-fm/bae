import AppKit
import BaeKit
import SwiftUI

/// One pressing row beneath a release-group card. Year-led headline (big
/// tabular) + label inline, a chip row of cat# (mono) · country · format, and
/// the signal badges that produced it. No cover and no per-row commit buttons:
/// the cover lives on the group card above, and clicking the row opens the
/// docked confirm pane where the Exact / Metadata-only choice is made.
///
/// Selecting a row that's already in the library still opens the pane — it
/// surfaces the "already imported" banner and leaves Import disabled there —
/// so the row stays tappable; it only dims to signal the dupe.
struct ImportSearchResultRow: View {
    let result: MetadataResult
    let isImporting: Bool
    let libraryStatus: BridgeLibraryStatus?
    /// Which signals produced/confirmed this result, for the badge row. `nil`
    /// for manual-search results (no auto-identify signals).
    var provenance: ResultProvenance?
    let isSelected: Bool
    let onSelect: () -> Void

    private var isInLibrary: Bool {
        libraryStatus?.releaseInLibrary == true
    }

    var body: some View {
        Button(action: onSelect) {
            HStack(alignment: .center, spacing: 10) {
                VStack(alignment: .leading, spacing: 3) {
                    headline
                    chipRow
                    signalBadges
                }
                .opacity(isInLibrary ? 0.55 : 1.0)
                Spacer(minLength: 8)
                libraryMarker
                chevron
            }
            .padding(.vertical, 9)
            .padding(.horizontal, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(rowBackground)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(isImporting)
    }

    private var rowBackground: some View {
        RoundedRectangle(cornerRadius: 8)
            .fill(isSelected ? Theme.accent.opacity(0.12) : .clear)
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(
                        isSelected ? Theme.accent.opacity(0.4) : .clear,
                        lineWidth: 1
                    )
            )
    }

    // MARK: - Headline

    private var headline: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            if let year = result.year {
                Text(String(year))
                    .font(.system(size: 16, weight: .semibold))
                    .monospacedDigit()
                    .foregroundStyle(.primary)
            }
            else {
                Text("Year unknown")
                    .font(.callout)
                    .foregroundStyle(.tertiary)
            }
            if let label = result.label {
                Text(label)
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
    }

    // MARK: - Chip row

    private var chipRow: some View {
        HStack(spacing: 7) {
            if let cat = result.catalogNumber {
                Text(cat)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(
                        .white.opacity(0.05),
                        in: RoundedRectangle(cornerRadius: 3)
                    )
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            if let country = result.country {
                Text(country)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            if result.format != nil
                && (result.catalogNumber != nil || result.country != nil)
            {
                Text(verbatim: "·")
                    .font(.caption2)
                    .foregroundStyle(.quaternary)
            }
            if let format = result.format {
                Text(format)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
    }

    // MARK: - Signal badges

    /// Which signals produced/confirmed this row. All three chips stay in the
    /// tree (opacity-toggled) so row height is stable across the list.
    @ViewBuilder
    private var signalBadges: some View {
        if let provenance {
            HStack(spacing: 4) {
                signalBadge(
                    "Disc ID",
                    icon: "opticaldiscdrive",
                    on: provenance.byDiscId
                )
                signalBadge(
                    "Barcode",
                    icon: "barcode",
                    on: provenance.byBarcode
                )
                signalBadge(
                    "Catalog",
                    icon: "tag",
                    on: provenance.matchesCatalog
                )
            }
            .padding(.top, 1)
        }
    }

    private func signalBadge(
        _ label: LocalizedStringKey,
        icon: String,
        on: Bool
    )
        -> some View
    {
        HStack(spacing: 3) {
            Image(systemName: icon)
            Text(label)
        }
        .font(.caption2.weight(.medium))
        .padding(.horizontal, 6)
        .padding(.vertical, 2)
        .background(Color.accentColor.opacity(0.15), in: Capsule())
        .foregroundStyle(Color.accentColor)
        .opacity(on ? 1 : 0)
        .allowsHitTesting(false)
        .accessibilityHidden(!on)
    }

    // MARK: - Trailing

    /// "In library" tag for an already-imported pressing. Kept in the tree
    /// (opacity-toggled) so selecting a row doesn't reflow the column.
    private var libraryMarker: some View {
        HStack(spacing: 4) {
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green)
            Text("In library")
        }
        .font(.caption2)
        .foregroundStyle(.tertiary)
        .opacity(isInLibrary ? 1 : 0)
        .allowsHitTesting(false)
        .accessibilityHidden(!isInLibrary)
    }

    private var chevron: some View {
        Image(systemName: "chevron.right")
            .font(.caption.weight(.semibold))
            .foregroundStyle(
                isSelected ? Theme.accent : Color.white.opacity(0.3)
            )
    }
}

// MARK: - Preview

#Preview("Pressing Rows") {
    VStack(spacing: 2) {
        ImportSearchResultRow(
            result: MetadataResult(
                bridge: BridgeMetadataResult(
                    source: .musicBrainz,
                    releaseId: "p-1",
                    year: 1992,
                    format: "CD",
                    label: "Label Name",
                    catalogNumber: "CAT 3922 CD",
                    country: "BE",
                )
            ),
            isImporting: false,
            libraryStatus: nil,
            provenance: ResultProvenance(
                byDiscId: true,
                byBarcode: false,
                matchesCatalog: true
            ),
            isSelected: true,
            onSelect: {},
        )
        ImportSearchResultRow(
            result: MetadataResult(
                bridge: BridgeMetadataResult(
                    source: .musicBrainz,
                    releaseId: "p-2",
                    year: 2012,
                    format: "2×Vinyl",
                    label: "Label Name",
                    catalogNumber: "CAT 92021 LP",
                    country: "UK",
                )
            ),
            isImporting: false,
            libraryStatus: BridgeLibraryStatus(
                releaseId: "p-2",
                releaseInLibrary: true,
                albumInLibrary: true,
                albumTitle: "Album Title",
                albumId: "album-1",
            ),
            provenance: nil,
            isSelected: false,
            onSelect: {},
        )
    }
    .padding()
    .frame(width: 520)
    .windowBackground()
    .environment(UiStore())
    .environment(MediaPaths.stub)
}
