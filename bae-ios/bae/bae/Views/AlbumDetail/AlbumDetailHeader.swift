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

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            ImageView(imageRef: detail.summary.cover, pointSize: 140)
                .frame(width: 140, height: 140)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .contentShape(Rectangle())
                .onTapGesture {
                    if !detail.galleryItems.isEmpty { showGallery = true }
                }
            VStack(alignment: .leading, spacing: 4) {
                Text(display.title)
                    .font(.title2.bold())
                if let metadata = display.albumMetadata,
                    !metadata.artistNames.isEmpty
                {
                    Text(metadata.artistNames)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                if let year = display.albumMetadata?.year {
                    Text(String(year))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if !detail.compactMetadata.isEmpty {
                    Text(detail.compactMetadata)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.top, 4)
                }
                playButtons
                    .padding(.top, 8)
                queueButtons
                    .padding(.top, 4)
            }
            Spacer(minLength: 0)
        }
    }

    private var playButtons: some View {
        HStack(spacing: 8) {
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
        HStack(spacing: 8) {
            Button {
                queue.addReleaseNext(releaseId)
            } label: {
                Label("Play Next", systemImage: "text.insert")
                    .font(.caption)
            }
            Button {
                queue.addReleaseToQueue(releaseId)
            } label: {
                Label("Add to Queue", systemImage: "text.append")
                    .font(.caption)
            }
        }
        .buttonStyle(.bordered)
        .tint(Theme.accent)
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
