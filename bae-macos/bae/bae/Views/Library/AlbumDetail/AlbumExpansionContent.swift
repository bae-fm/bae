import BaeKit
import SwiftUI

/// The expanded album-detail card: cover, title block, release picker, primary
/// actions, and the track list. Pure prop-driven content — its wiring parent
/// `AlbumDetailView` owns the state and supplies every callback.
struct AlbumExpansionContent: View {
    let summary: AlbumSummary
    /// Fat detail for the release the user is currently viewing.
    let selectedRelease: ReleaseDetail
    let lightboxItems: [LightboxItem]
    /// Cursor over the album's releases. Drives the release picker and
    /// guarantees a valid selection on every read.
    @Binding
    var releaseCursor: Cursor<ReleaseRef>
    let currentTrackId: String?
    /// The id of the track currently loading (cloud download / decode warm-up),
    /// or nil. The matching row shows a spinner where its play/speaker glyph goes.
    let loadingTrackId: String?
    let isPlaying: Bool
    let onClose: () -> Void
    let onPlay: () -> Void
    let onShuffle: () -> Void
    let onPlayFromTrack: (Int) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onAddNextAlbum: () -> Void
    let onAddAlbumToQueue: () -> Void
    let onChangeCover: () -> Void
    let onEditMetadata: () -> Void
    let onReIdentify: () -> Void
    let onManage: () -> Void
    let onExportRelease: () -> Void
    let onSaveReleaseAs: () -> Void
    let onSetPrimaryRelease: () -> Void
    let onDeleteRelease: () -> Void
    let onExportTrack: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 36) {
                albumArt
                    .frame(width: 340, height: 340)
                    .clipShape(RoundedRectangle(cornerRadius: 14))
                    .shadow(color: .black.opacity(0.6), radius: 20, y: 12)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        if !lightboxItems.isEmpty {
                            uiStore.presentLightbox(items: lightboxItems)
                        }
                    }
                VStack(alignment: .leading, spacing: 4) {
                    Text(summary.title)
                        .font(.system(size: 30, weight: .heavy))
                        .tracking(-0.5)
                        .lineLimit(1)
                    HStack(spacing: 8) {
                        Text(summary.artistNames)
                            .foregroundStyle(.secondary)
                        if let year = summary.year {
                            Text(verbatim: "\u{00B7}")
                                .foregroundStyle(.tertiary)
                            Text(String(year))
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                    if releaseCursor.canCycle {
                        releasePicker
                    }
                    Text(selectedRelease.compactMetadata)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.tertiary)
                    HStack(spacing: 10) {
                        Button(action: onPlay) {
                            HStack(spacing: 7) {
                                Image(systemName: "play.fill")
                                    .font(.system(size: 12, weight: .bold))
                                Text("Play")
                                    .font(.system(size: 15, weight: .bold))
                            }
                            .foregroundStyle(.white)
                            .padding(.horizontal, 20)
                            .padding(.vertical, 9)
                            .background(
                                Theme.accent,
                                in: RoundedRectangle(cornerRadius: 10)
                            )
                            .shadow(
                                color: Theme.accent.opacity(0.22),
                                radius: 5,
                                y: 3
                            )
                        }
                        .buttonStyle(.plain)
                        albumMenu
                    }
                    .padding(
                        EdgeInsets(top: 12, leading: 0, bottom: 0, trailing: 0)
                    )
                    AlbumTrackListView(
                        release: selectedRelease,
                        isCompilation: summary.isCompilation,
                        currentTrackId: currentTrackId,
                        loadingTrackId: loadingTrackId,
                        isPlaying: isPlaying,
                        onPlayFromTrack: onPlayFromTrack,
                        onTogglePlayPause: onTogglePlayPause,
                        onAddNext: onAddNext,
                        onAddToQueue: onAddToQueue,
                        onExportTrack: onExportTrack,
                    )
                    .padding(.top, 18)
                }
            }
        }
        .padding(32)
        .background(
            LinearGradient(
                colors: [Theme.surfaceElevated, Theme.surface],
                startPoint: .top,
                endPoint: .bottom
            ),
            in: RoundedRectangle(cornerRadius: 18)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 18)
                .strokeBorder(.white.opacity(0.08), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.45), radius: 28, y: 18)
        .overlay(alignment: .topTrailing) {
            Button {
                withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) {
                    onClose()
                }
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 30, height: 30)
                    .background(
                        .white.opacity(0.06),
                        in: RoundedRectangle(cornerRadius: 9)
                    )
            }
            .buttonStyle(.plain)
            .padding(16)
        }
    }

    private var canSetAsPrimaryRelease: Bool {
        selectedRelease.id != summary.primaryReleaseId
    }

    private var albumMenu: some View {
        Menu {
            Button(action: onPlay) {
                Label("Play", systemImage: "play.fill")
            }
            Button(action: onShuffle) {
                Label("Shuffle", systemImage: "shuffle")
            }
            Divider()
            Button(action: { onAddNextAlbum() }) {
                Label(
                    "Play Next",
                    systemImage: "text.line.first.and.arrowtriangle.forward"
                )
            }
            Button(action: { onAddAlbumToQueue() }) {
                Label("Add to Queue", systemImage: "text.append")
            }
            Divider()
            Button("Change Cover...") { onChangeCover() }
            Button("Edit metadata...") { onEditMetadata() }
            Button("Re-identify...") { onReIdentify() }
            Button("Storage...") { onManage() }
            Button("Export…") { onExportRelease() }
            Button("Save As…") { onSaveReleaseAs() }
            if releaseCursor.canCycle, canSetAsPrimaryRelease {
                Divider()
                Button("Set as Primary Release") { onSetPrimaryRelease() }
            }
            Divider()
            Button(role: .destructive, action: onDeleteRelease) {
                Label("Delete Release", systemImage: "trash")
            }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 36, height: 36)
                .background(
                    .white.opacity(0.07),
                    in: RoundedRectangle(cornerRadius: 10)
                )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    private var albumArt: some View {
        ImageView(imageRef: selectedRelease.summary.cover, pointSize: 340)
    }

    private var releasePicker: some View {
        NativeSegmentedControl(
            selectedIndex: Binding(
                get: { releaseCursor.index },
                set: { newIndex in
                    // Guard against an out-of-range index from AppKit's segmented
                    // control. `Cursor.select(id:)` is a no-op for unknown ids.
                    guard releaseCursor.items.indices.contains(newIndex) else {
                        return
                    }
                    releaseCursor.select(id: releaseCursor.items[newIndex].id)
                },
            ),
            segments: releaseCursor.items.enumerated()
                .map { index, ref in
                    NativeSegmentedControl.Segment(
                        label: libraryStore.releaseDetails[ref.id]?.displayName
                            ?? String(localized: "Release \(index + 1)"),
                        systemImage: releaseContainsCurrentTrack(id: ref.id)
                            ? "speaker.fill" : nil,
                    )
                },
        )
        .padding(EdgeInsets(top: 4, leading: 0, bottom: 2, trailing: 0))
    }

    private func releaseContainsCurrentTrack(id: String) -> Bool {
        guard let currentTrackId else {
            return false
        }
        guard let detail = libraryStore.releaseDetails[id] else {
            return false
        }
        return detail.tracks.contains(where: { $0.id == currentTrackId })
    }
}

#if DEBUG
    @MainActor
    private func previewExpansion(
        albumId: String,
        currentTrackId: String? = nil,
        loadingTrackId: String? = nil,
        isPlaying: Bool = false,
    ) -> some View {
        let store = LibraryStore()
        guard let album = PreviewData.albumDetails[albumId] else {
            fatalError("no preview album for id: \(albumId)")
        }
        store.internAlbumDetail(album)
        guard let summary = store.albumSummaries[albumId] else {
            fatalError(
                "preview seed did not create summary for id: \(albumId)"
            )
        }
        guard let primary = store.releaseDetails[summary.primaryReleaseId]
        else {
            fatalError(
                "no releaseDetail seeded for id: \(summary.primaryReleaseId)"
            )
        }
        let cursor = PreviewData.releaseCursor(
            releaseIds: summary.releaseIds,
            preferring: summary.primaryReleaseId
        )
        return
            PreviewData.albumExpansionContent(
                summary: summary,
                selectedRelease: primary,
                releaseCursor: .constant(cursor),
                currentTrackId: currentTrackId,
                loadingTrackId: loadingTrackId,
                isPlaying: isPlaying,
            )
            .padding()
            .frame(width: 1100)
            .background(Theme.background)
            .environment(UiStore())
            .environment(store)
    }

    #Preview("Single Disc") {
        previewExpansion(
            albumId: "a-01",
            currentTrackId: "t-d1-2",
            isPlaying: true
        )
    }

    #Preview("Single Disc — Track Loading") {
        previewExpansion(
            albumId: "a-01",
            currentTrackId: "t-d1-2",
            loadingTrackId: "t-d1-3",
            isPlaying: true
        )
    }

    #Preview("Vinyl — Two Sides") {
        previewExpansion(
            albumId: "a-21",
            currentTrackId: "t-d2-3",
            isPlaying: true
        )
    }

    #Preview("CD — Two Discs") {
        previewExpansion(albumId: "a-22")
    }

    private struct MultiReleasePreview: View {
        @State
        private var selectedReleaseId: String = "rel-a-04-0"
        @State
        private var store = LibraryStore()

        var body: some View {
            Group {
                if let summary = store.albumSummaries["a-04"],
                    let selected = store.releaseDetails[selectedReleaseId]
                {
                    PreviewData.albumExpansionContent(
                        summary: summary,
                        selectedRelease: selected,
                        // Live cursor so selecting a release in the picker cycles
                        // the preview to that release's detail.
                        releaseCursor: Binding(
                            get: {
                                PreviewData.releaseCursor(
                                    releaseIds: summary.releaseIds,
                                    preferring: selectedReleaseId
                                )
                            },
                            set: { selectedReleaseId = $0.current.id },
                        ),
                        currentTrackId: "t-d2-3",
                        isPlaying: true,
                    )
                    .padding()
                    .frame(width: 1100, height: 700, alignment: .top)
                    .background(Theme.background)
                    .environment(UiStore())
                    .environment(store)
                    .environment(MediaPaths.stub)
                }
            }
            .onAppear { seedIfNeeded() }
        }

        @MainActor
        private func seedIfNeeded() {
            if store.albumSummaries["a-04"] == nil {
                guard let album = PreviewData.albumDetails["a-04"] else {
                    fatalError("a-04 not in PreviewData.albumDetails")
                }
                store.internAlbumDetail(album)
            }
        }
    }

    #Preview("Multiple Releases") {
        MultiReleasePreview()
            .environment(MediaPaths.stub)
            .environment(UiStore())
            .environment(LibraryStore())
    }
#endif
