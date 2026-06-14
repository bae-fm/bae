import AppKit
import SwiftUI

/// Row in the identify pipeline's candidate list. Two visual shapes:
///
/// - `.full` (default) — title-led headline + flowing chip row of
///   artist · year · format · label · cat# · country. Used in `.found`
///   and manual-search lists where each row is a candidate release the
///   user is comparing.
/// - `.pressing` — year-led headline (big tabular) + label inline +
///   chip row of cat# (mono) · country · format. Drops the redundant
///   title/artist since every row in a conflict section is a pressing
///   of the same album.
///
/// Two per-row commit affordances:
///
/// - **Exact pressing** — commits Exact: identity row carries the
///   picked release ID, pressing metadata seeds from the picked
///   release.
/// - **Metadata** — commits Approximate: identity row carries the
///   group only (no release ID), pressing metadata stays NULL — only
///   album-group-stable fields seed.
///
/// Both buttons fan into the same prefetch-and-confirm flow; only the
/// `IdentityChoice` differs. Disabled when the release is already in
/// the library, or when an import is in progress for this candidate.
struct ImportSearchResultRow: View {
    enum Kind {
        case full
        case pressing
    }

    let result: MetadataResult
    var kind: Kind = .full
    let isImporting: Bool
    let libraryStatus: LibraryStatus?
    /// Which signals produced/confirmed this result, for the badge row. `nil`
    /// for manual-search results (no auto-identify signals).
    var provenance: ResultProvenance?
    let onCommit: (IdentityChoice) -> Void

    @Environment(UiStore.self)
    private var uiStore
    @State
    private var titleHovered = false

    private var commitDisabled: Bool {
        libraryStatus?.releaseInLibrary == true || isImporting
    }

    private var isInLibrary: Bool {
        libraryStatus?.releaseInLibrary == true
    }

    private var coverSize: CGFloat {
        switch kind {
        case .full: 64
        case .pressing: 48
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            cover
            VStack(alignment: .leading, spacing: 3) {
                headline
                chipRow
                signalBadges
            }
            .opacity(isInLibrary ? 0.55 : 1.0)
            Spacer(minLength: 8)
            trailing
        }
        .padding(.vertical, 6)
    }

    // MARK: - Cover

    private var cover: some View {
        ImageView(
            source: result.coverImageSource,
            pointSize: coverSize
        )
        .frame(width: coverSize, height: coverSize)
        .clipShape(RoundedRectangle(cornerRadius: 4))
        .overlay(
            RoundedRectangle(cornerRadius: 4)
                .stroke(.white.opacity(0.06), lineWidth: 1)
        )
        .opacity(isInLibrary ? 0.5 : 1.0)
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
        _ label: String,
        icon: String,
        on: Bool
    ) -> some View {
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

    // MARK: - Headline

    @ViewBuilder
    private var headline: some View {
        switch kind {
        case .full:
            HStack(spacing: 6) {
                Text(result.title)
                    .font(.headline)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if result.releaseUrl != nil {
                    Image(systemName: "arrow.up.right.square")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .opacity(titleHovered ? 1 : 0)
                }
            }
            .animation(.easeInOut(duration: 0.1), value: titleHovered)
            .onHover { titleHovered = $0 }
            .contentShape(Rectangle())
            .onTapGesture { openReleaseUrl() }
        case .pressing:
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                if let year = result.year {
                    Text(String(year))
                        .font(
                            .system(.title3, design: .default).weight(.semibold)
                        )
                        .monospacedDigit()
                }
                else {
                    Text("Year unknown")
                        .font(.callout)
                        .foregroundStyle(.tertiary)
                }
                if let label = result.label {
                    Text(label)
                        .font(.callout)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
        }
    }

    // MARK: - Chip row

    @ViewBuilder
    private var chipRow: some View {
        switch kind {
        case .full:
            FlowingChips(parts: fullChipParts)
        case .pressing:
            HStack(spacing: 6) {
                if let cat = result.catalogNumber {
                    Text(cat)
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 5)
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
                    Text("·")
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
    }

    private var fullChipParts: [(String, Bool)] {
        var parts: [(String, Bool)] = []
        if let artist = result.artist { parts.append((artist, false)) }
        if let year = result.year { parts.append((String(year), true)) }
        if let format = result.format { parts.append((format, false)) }
        if let label = result.label { parts.append((label, false)) }
        if let cat = result.catalogNumber { parts.append((cat, true)) }
        if let country = result.country { parts.append((country, false)) }
        return parts
    }

    // MARK: - Trailing

    @ViewBuilder
    private var trailing: some View {
        if isInLibrary {
            VStack(alignment: .trailing, spacing: 3) {
                if let albumId = libraryStatus?.albumId {
                    Button("View in Library") {
                        uiStore.navigateToAlbum(albumId)
                    }
                    .buttonStyle(.link)
                }
                HStack(spacing: 4) {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                    Text("Imported")
                }
                .foregroundStyle(.tertiary)
            }
            .font(.caption)
        }
        else {
            VStack(alignment: .trailing, spacing: 4) {
                if let status = libraryStatus, status.albumInLibrary {
                    if let albumId = status.albumId {
                        Button("View in Library") {
                            uiStore.navigateToAlbum(albumId)
                        }
                        .buttonStyle(.link)
                        .font(.caption)
                    }
                    Text("Another release")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                HStack(spacing: 6) {
                    Button("Exact pressing") {
                        onCommit(
                            .exact(
                                releaseId: result.releaseId,
                                source: result.source
                            )
                        )
                    }
                    .controlSize(.small)
                    .disabled(commitDisabled)
                    .help("Commit as the exact pressing")
                    Button("Metadata") {
                        onCommit(
                            .approximate(
                                releaseId: result.releaseId,
                                source: result.source
                            )
                        )
                    }
                    .controlSize(.small)
                    .disabled(commitDisabled)
                    .help("Use only for metadata; leave pressing fields blank")
                }
            }
        }
    }

    private func openReleaseUrl() {
        if let urlStr = result.releaseUrl,
            let url = URL(string: urlStr)
        {
            NSWorkspace.shared.open(url)
        }
    }
}

// MARK: - FlowingChips

/// Mid-dot-separated, wrap-friendly chip row for the `.full` row's
/// secondary line. Tabular numerics on numeric parts (year, cat#)
/// keep columns visually aligned across rows.
private struct FlowingChips: View {
    let parts: [(String, Bool)]

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            ForEach(Array(parts.enumerated()), id: \.offset) { idx, part in
                if idx > 0 {
                    Text("·")
                        .font(.caption2)
                        .foregroundStyle(.quaternary)
                }
                Text(part.0)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
    }
}

// MARK: - Preview

#Preview("Search Result Row") {
    VStack(spacing: 12) {
        ImportSearchResultRow(
            result: MetadataResult(
                bridge: BridgeMetadataResult(
                    source: .musicBrainz,
                    releaseId: "rel-123",
                    title: "Album Title",
                    artist: "Artist Name",
                    year: 2024,
                    format: "CD",
                    label: "Label Name",
                    catalogNumber: "CAT-001",
                    country: "US",
                    coverUrl: nil,
                    sourceGroupId: nil,
                    releaseUrl: "https://musicbrainz.org/release/rel-123",
                )
            ),
            isImporting: false,
            libraryStatus: nil,
            onCommit: { _ in },
        )
        ImportSearchResultRow(
            result: MetadataResult(
                bridge: BridgeMetadataResult(
                    source: .musicBrainz,
                    releaseId: "rel-456",
                    title: "Album Title",
                    artist: "Artist Name",
                    year: 1996,
                    format: "CD",
                    label: "Label Name",
                    catalogNumber: "CAT-002",
                    country: "US",
                    coverUrl: nil,
                    sourceGroupId: nil,
                    releaseUrl: nil,
                )
            ),
            isImporting: false,
            libraryStatus: LibraryStatus(
                bridge: BridgeLibraryStatus(
                    releaseId: "rel-456",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "Album Title",
                    albumId: "album-123",
                )
            ),
            onCommit: { _ in },
        )
        ImportSearchResultRow(
            result: MetadataResult(
                bridge: BridgeMetadataResult(
                    source: .musicBrainz,
                    releaseId: "rel-789",
                    title: "Album Title",
                    artist: "Artist Name",
                    year: 2010,
                    format: "12\" Vinyl · Reissue",
                    label: "Label Name",
                    catalogNumber: "CAT-003",
                    country: "UK",
                    coverUrl: nil,
                    sourceGroupId: nil,
                    releaseUrl: nil,
                )
            ),
            isImporting: false,
            libraryStatus: LibraryStatus(
                bridge: BridgeLibraryStatus(
                    releaseId: "rel-789",
                    releaseInLibrary: false,
                    albumInLibrary: true,
                    albumTitle: "Album Title",
                    albumId: "album-456",
                )
            ),
            onCommit: { _ in },
        )
        Divider()
        ImportSearchResultRow(
            result: MetadataResult(
                bridge: BridgeMetadataResult(
                    source: .musicBrainz,
                    releaseId: "p-1",
                    title: "Black Sabbath",
                    artist: "Black Sabbath",
                    year: 1987,
                    format: "CD",
                    label: "Creative Sounds Ltd.",
                    catalogNumber: "6006-6, 449805-2",
                    country: "DE",
                    coverUrl: nil,
                    sourceGroupId: nil,
                    releaseUrl: nil,
                )
            ),
            kind: .pressing,
            isImporting: false,
            libraryStatus: nil,
            onCommit: { _ in },
        )
        ImportSearchResultRow(
            result: MetadataResult(
                bridge: BridgeMetadataResult(
                    source: .musicBrainz,
                    releaseId: "p-2",
                    title: "Black Sabbath",
                    artist: "Black Sabbath",
                    year: nil,
                    format: "CD · Album · Reissue",
                    label: "Creative Sounds",
                    catalogNumber: "6006-2",
                    country: "Unknown",
                    coverUrl: nil,
                    sourceGroupId: nil,
                    releaseUrl: nil,
                )
            ),
            kind: .pressing,
            isImporting: false,
            libraryStatus: nil,
            onCommit: { _ in },
        )
    }
    .padding()
    .environment(UiStore())
    .environment(MediaPaths.stub)
}

private struct ImportSearchCoverStatePreview: View {
    var body: some View {
        HStack(spacing: 12) {
            coverState(.unavailable)
            coverState(.loading)
            coverState(.failed)
        }
        .padding()
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }

    private func coverState(_ reason: PlaceholderReason) -> some View {
        ImagePlaceholderView(reason: reason, pointSize: 64)
            .frame(width: 64, height: 64)
    }
}

#Preview("Search Result Cover States") {
    ImportSearchCoverStatePreview()
}
