import BaeKit
import SwiftUI

struct PreviewOverlay: View {
    @Environment(PreviewAudio.self)
    var previewAudio
    let path: String
    let isPlaying: Bool
    let durationMs: UInt64

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
                        PreviewProgressRepresentable(
                            durationMs: durationMs,
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
            durationMs: 210_000,
        )
        .frame(width: 600, height: 400)
        .environment(PreviewAudio.stub)
        .windowBackground()
    }
#endif
