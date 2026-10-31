import BaeKit
import SwiftUI

struct PreviewOverlay: View {
    @Environment(PreviewAudio.self)
    var previewAudio
    let path: Data
    let isPlaying: Bool

    var body: some View {
        ModalOverlay(
            onDismiss: {
                previewAudio.previewStop()
            },
            content: {
                VStack(spacing: ThemeSpace.group) {
                    HStack {
                        Text(
                            URL(filePathBytes: path, isDirectory: false)
                                .lastPathComponent
                        )
                        .themeText(.body)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        Spacer()
                        Button {
                            previewAudio.previewStop()
                        } label: {
                            Image(systemName: "xmark")
                                .themeIcon(.medium)
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
                            .themeIcon(.medium)
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
            path: Data("/Music/Downloads/Album Title/01 Track Title.flac".utf8),
            isPlaying: true,
        )
        .frame(width: 600, height: 400)
        .environment(PreviewAudio.stub())
        .environment(
            \.previewPosition,
            PreviewData.previewPosition(positionMs: 52_500, durationMs: 210_000)
        )
        .windowBackground()
    }
#endif
