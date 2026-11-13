import BaeKit
import SwiftUI

/// The expanded album-detail card; `AlbumDetailView` owns the state and
/// supplies every callback.
struct AlbumExpansionContent: View {
    /// The album and its releases as its detail read returned them; the
    /// release picker names each release from here.
    let detail: AlbumDetail
    /// Fat detail for the release the user is currently viewing.
    let selectedRelease: ReleaseDetail
    let onBrowseImages: () -> Void
    /// Cursor over the album's releases, one item per release of
    /// `detail.releases` in its order; drives the release picker.
    @Binding
    var releaseCursor: Cursor<ReleaseRef>
    let currentTrackId: String?
    let isPlaying: Bool
    let onClose: () -> Void
    let onPlay: () -> Void
    let onShuffle: () -> Void
    let onPlayFromTrack: (String) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onAddNextAlbum: () -> Void
    let onAddAlbumToQueue: () -> Void
    /// Open the release editor: cover, fields, album and catalog search.
    let onEdit: () -> Void
    let onOpenStorage: () -> Void
    let onExportRelease: () -> Void
    let onSaveReleaseAs: () -> Void
    let onSetPrimaryRelease: () -> Void
    let onDeleteRelease: () -> Void
    let onDeleteAlbum: () -> Void
    let onExportTrack: (String) -> Void

    /// The album cover's side.
    private static let coverSize: CGFloat = 340

    private var album: BridgeAlbum { detail.album }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: ThemeSpace.page) {
                albumArt
                    .frame(width: Self.coverSize, height: Self.coverSize)
                    .clipShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.cover)
                    )
                    .shadow(color: Theme.shadow, radius: 20, y: 12)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onBrowseImages)
                VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                    Text(album.title)
                        .themeText(.hero)
                        .lineLimit(1)
                    HStack(spacing: ThemeSpace.related) {
                        Text(album.artistLine.text)
                            .foregroundStyle(.secondary)
                        if let year = album.year {
                            Text(verbatim: FactList.glyph)
                                .foregroundStyle(.tertiary)
                            Text(PressingText.year(year))
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .themeText(.heading)
                    .lineLimit(1)
                    if releaseCursor.canCycle {
                        releasePicker
                    }
                    ReleaseFactsLine(
                        pressingLine: selectedRelease.pressingLine,
                        labelsLine: selectedRelease.labelsLine,
                        records: selectedRelease.records
                    )
                    // Why this Mac can't play the release: the device its
                    // audio is on, or its originals missing here.
                    if let unplayable = selectedRelease.unplayableLine {
                        Text(unplayable)
                            .themeText(.detail)
                            .foregroundStyle(.secondary)
                    }
                    HStack(spacing: ThemeSpace.related) {
                        Button("Play", action: onPlay)
                            .buttonStyle(PrimaryButtonStyle())
                        albumMenu
                    }
                    .padding(.top, ThemeSpace.group)
                    AlbumTrackListView(
                        release: selectedRelease,
                        isCompilation: album.isCompilation,
                        currentTrackId: currentTrackId,
                        isPlaying: isPlaying,
                        onPlayFromTrack: onPlayFromTrack,
                        onTogglePlayPause: onTogglePlayPause,
                        onAddNext: onAddNext,
                        onAddToQueue: onAddToQueue,
                        onExportTrack: onExportTrack,
                    )
                    .padding(.top, ThemeSpace.edge)
                }
            }
        }
        .padding(ThemeSpace.page)
        .background(
            Theme.surfaceElevated,
            in: RoundedRectangle(cornerRadius: ThemeRadius.panel)
        )
        .overlay(
            RoundedRectangle(cornerRadius: ThemeRadius.panel)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        )
        .shadow(color: Theme.shadow, radius: 28, y: 18)
        .overlay(alignment: .topTrailing) {
            MacCloseButton(onClose: onClose)
                .padding(ThemeSpace.edge)
        }
    }

    private var canSetAsPrimaryRelease: Bool {
        selectedRelease.id != album.primaryReleaseId
    }

    private var albumMenu: some View {
        MacMenuButton {
            MacMenuItem("Play", action: onPlay)
            MacMenuItem("Shuffle", action: onShuffle)
            MacMenuDivider()
            MacMenuItem("Play next") { onAddNextAlbum() }
            MacMenuItem("Add to queue") { onAddAlbumToQueue() }
            MacMenuDivider()
            MacMenuItem("Edit…") { onEdit() }
            MacMenuItem("Storage…") { onOpenStorage() }
            MacMenuItem("Export…") { onExportRelease() }
            MacMenuItem("Save as…") { onSaveReleaseAs() }
            if releaseCursor.canCycle, canSetAsPrimaryRelease {
                MacMenuDivider()
                MacMenuItem("Set as primary release") { onSetPrimaryRelease() }
            }
            MacMenuDivider()
            MacMenuItem("Delete release", action: onDeleteRelease)
            MacMenuItem("Delete album…", action: onDeleteAlbum)
        } label: {
            Image(systemName: "ellipsis")
                .themeIcon(.medium)
                .foregroundStyle(.secondary)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .background(
                    Theme.hover,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
        }
        .fixedSize()
    }

    private var albumArt: some View {
        ImageView(
            imageRef: selectedRelease.summary.cover,
            pointSize: Self.coverSize
        )
    }

    private var releasePicker: some View {
        MacSegmentedControl(
            selectedIndex: Binding(
                get: { releaseCursor.index },
                set: { newIndex in
                    // Releases can change while a click is on its way.
                    guard releaseCursor.items.indices.contains(newIndex) else {
                        return
                    }
                    releaseCursor.select(id: releaseCursor.items[newIndex].id)
                },
            ),
            segments: pickerReleases.map { release in
                MacSegmentedControl.Segment(
                    label: release.displayName,
                    systemImage: release.tracks.contains {
                        $0.id == currentTrackId
                    } ? "speaker.fill" : nil,
                )
            },
        )
        .padding(.top, ThemeSpace.inline)
        .padding(.bottom, ThemeSpace.line)
    }

    /// The releases the picker's segments name, one per cursor item. The
    /// cursor is built over `detail.releases`; one built over anything else
    /// would name the wrong release under a segment.
    private var pickerReleases: [ReleaseDetail] {
        precondition(
            releaseCursor.items.map(\.id) == detail.releases.map(\.id),
            "The release cursor is not over the album's releases"
        )
        return detail.releases
    }
}

/// The release's facts on two lines, each cut short on its own: the pressing,
/// then its labels. When a catalog describes the release, an arrow beside the
/// first line marks them and a click toggles a card naming the catalogs.
///
/// The card is drawn in the window rather than as a popover, so it carries its
/// own monitor that closes it on a click away or Escape.
struct ReleaseFactsLine: View {
    let pressingLine: String
    let labelsLine: String
    let records: [BridgeReleaseRecord]

    /// How far under the line the card's top sits.
    private static let cardOffset = ThemeSpace.related

    @State
    private var isHovering = false
    @State
    private var isShowingCard = false
    @State
    private var trigger = OverlayAnchor()
    /// The facts' own height, which is where the card hangs from.
    @State
    private var lineHeight: CGFloat = 0

    var body: some View {
        if records.isEmpty {
            factsText
        }
        else {
            Button {
                isShowingCard.toggle()
            } label: {
                HStack(
                    alignment: .firstTextBaseline,
                    spacing: ThemeSpace.inline
                ) {
                    factsText
                    Image(systemName: "arrow.up.right")
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .allowsHitTesting(false)
                        .accessibilityLabel(
                            localizedCoreString("core.identity.identified")
                        )
                }
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.line)
                .background(
                    RoundedRectangle(cornerRadius: ThemeRadius.chip)
                        .fill(isHovering ? Theme.hover : Color.clear)
                )
                // The fill bleeds outward without moving the text.
                .padding(.horizontal, -ThemeSpace.compact)
                .padding(.vertical, -ThemeSpace.line)
            }
            .buttonStyle(.accentPlain)
            .background { OverlayTrigger(anchor: trigger) }
            .onHover { isHovering = $0 }
            .accessibilityIdentifier("release-facts")
            .onGeometryChange(for: CGFloat.self) { geometry in
                geometry.size.height
            } action: {
                lineHeight = $0
            }
            .overlay(alignment: .topLeading) {
                if isShowingCard {
                    card.offset(y: lineHeight + Self.cardOffset)
                }
            }
            // Over the rows that follow the line, which the card lies across.
            .zIndex(1)
        }
    }

    private var card: some View {
        ReleaseRecordsCard(records: records)
            .background(
                Theme.tile,
                in: RoundedRectangle(cornerRadius: ThemeRadius.control)
            )
            .overlay {
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .strokeBorder(Theme.hairline, lineWidth: 1)
            }
            .shadow(color: Theme.shadow, radius: 14, y: 8)
            .background {
                OverlayDismissMonitor(trigger: trigger) {
                    isShowingCard = false
                }
            }
            .fixedSize()
            .accessibilityIdentifier("release-facts-card")
    }

    /// The lines the release states, each on one line of its own.
    var shownLines: [String] {
        [pressingLine, labelsLine].filter { !$0.isEmpty }
    }

    private var factsText: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            ForEach(Array(shownLines.enumerated()), id: \.offset) { _, line in
                Text(line)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
        .themeText(.detail)
        .foregroundStyle(.tertiary)
    }
}
