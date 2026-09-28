import BaeKit
import SwiftUI

/// The album-detail header: cover (tap to open the gallery), title/artist/year
/// and compact metadata, and the play / shuffle / queue buttons for the shown
/// release.
struct AlbumDetailHeader: View {
    let display: AlbumDetailDisplay
    let releaseId: String
    let detail: ReleaseDetail
    @Binding
    var showGallery: Bool

    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue

    private static let coverSize: CGFloat = 140

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            metadata
            playButtons
            queueButtons
        }
    }

    private var metadata: some View {
        HStack(alignment: .top, spacing: ThemeSpace.edge) {
            ImageView(imageRef: detail.summary.cover, pointSize: Self.coverSize)
                .frame(width: Self.coverSize, height: Self.coverSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
                .contentShape(Rectangle())
                .onTapGesture {
                    if !detail.galleryItems.isEmpty { showGallery = true }
                }
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(display.title)
                    .themeText(.hero)
                if let metadata = display.albumMetadata,
                    !metadata.artistNames.isEmpty
                {
                    Text(metadata.artistNames)
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                }
                if let year = display.albumMetadata?.year {
                    Text(String(year))
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
                // The pressing, then its labels, each on one line of its
                // own.
                let factLines = [detail.pressingLine, detail.labelsLine]
                    .filter { !$0.isEmpty }
                if !factLines.isEmpty {
                    VStack(alignment: .leading, spacing: ThemeSpace.line) {
                        ForEach(Array(factLines.enumerated()), id: \.offset) {
                            _,
                            line in
                            Text(line)
                                .lineLimit(1)
                                .truncationMode(.tail)
                        }
                    }
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .padding(.top, ThemeSpace.inline)
                }
            }
            Spacer(minLength: 0)
        }
    }

    private var playButtons: some View {
        AlbumActionRow {
            Button {
                playback.playRelease(releaseId, nil, false)
            } label: {
                Label("Play", systemImage: "play.fill")
            }
            .buttonStyle(PrimaryButtonStyle())
            Button {
                playback.playRelease(releaseId, nil, true)
            } label: {
                Label("Shuffle", systemImage: "shuffle")
            }
            .buttonStyle(.bordered)
        }
    }

    private var queueButtons: some View {
        AlbumActionRow {
            Button {
                queue.addReleaseNext(releaseId)
            } label: {
                Label("Play Next", systemImage: "text.insert")
                    .themeText(.detail)
            }
            Button {
                queue.addReleaseToQueue(releaseId)
            } label: {
                Label("Add to Queue", systemImage: "text.append")
                    .themeText(.detail)
            }
        }
        .buttonStyle(.bordered)
        .tint(Theme.accent)
    }
}

/// Actions use a row when their labels fit and a column at larger text sizes.
private struct AlbumActionRow<Content: View>: View {
    @ViewBuilder
    var content: Content

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: ThemeSpace.related) { content }.fixedSize()
            VStack(alignment: .leading, spacing: ThemeSpace.related) { content }
        }
    }
}

#if DEBUG
#Preview {
    AlbumDetailHeader(
        display: AlbumDetailDisplay(summary: PreviewData.albumSummary),
        releaseId: "rel-a-1",
        detail: PreviewData.releaseDetail,
        showGallery: .constant(false)
    )
    .previewStores()
}
#endif
