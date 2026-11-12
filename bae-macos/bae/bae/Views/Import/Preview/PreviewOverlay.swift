import BaeKit
import SwiftUI

struct PreviewOverlay: View {
    @Environment(PreviewAudio.self)
    var previewAudio
    /// The previewed file's name, as core shows it.
    let name: String
    let isPlaying: Bool

    var body: some View {
        ModalOverlay(
            onDismiss: {
                previewAudio.previewStop()
            },
            content: {
                VStack(spacing: ThemeSpace.group) {
                    HStack {
                        Text(name)
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
                        .buttonStyle(.accentPlain)
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
                        .buttonStyle(.accentPlain)
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

#endif
