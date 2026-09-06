import BaeKit
import Combine
import SwiftUI

struct PreviewOverlay: View {
    @Environment(PreviewAudio.self)
    var previewAudio
    let path: String
    let isPlaying: Bool

    var body: some View {
        ModalOverlay(
            onDismiss: {
                previewAudio.previewStop()
            },
            content: {
                VStack(spacing: 12) {
                    HStack {
                        Text(URL(fileURLWithPath: path).lastPathComponent)
                            .font(.callout)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer()
                        Button {
                            previewAudio.previewStop()
                        } label: {
                            Image(systemName: "xmark")
                                .font(.body)
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                    }
                    HStack {
                        Button {
                            previewAudio.previewTogglePause()
                        } label: {
                            Image(
                                systemName: isPlaying
                                    ? "pause.fill" : "play.fill"
                            )
                            .font(.body)
                        }
                        .buttonStyle(.plain)
                        PreviewProgressView(
                            onSeek: { previewAudio.previewSeekByRatio($0) },
                        )
                        .frame(height: 20)
                    }
                }
                .padding()
                .frame(width: 400)
                .background(Theme.surface)
            },
        )
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Preview overlay") {
        PreviewOverlay(
            path: "/Music/Downloads/Album Title/01 Track Title.flac",
            isPlaying: true,
        )
        .frame(width: 600, height: 400)
        .environment(PreviewAudio.stub())
        .environment(
            \.previewProgressPublisher,
            Just(
                PlaybackPositionEvent.position(
                    progress: 0.25,
                    positionMs: 52_500,
                    durationMs: 210_000
                )
            )
            .eraseToAnyPublisher()
        )
        .windowBackground()
    }
#endif
