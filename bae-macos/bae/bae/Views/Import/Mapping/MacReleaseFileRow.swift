import BaeKit
import SwiftUI

/// One file of a release on one line: its name in mono, the damage after the
/// name in the warning colour behind the warning glyph when the file is
/// damaged, and its size at the right. Given `onOpen`, the whole line is one
/// button that opens the file. Given `onPlay`, an audio file's Play button
/// sits before the size, and a Stop button replaces it while `isPlaying`.
struct MacReleaseFileRow: View {
    let name: String
    let size: String
    var damage: String?
    var onOpen: (() -> Void)?
    var onPlay: (() -> Void)?
    var isPlaying = false

    var body: some View {
        if let onOpen {
            Button(action: onOpen) {
                content
            }
            .buttonStyle(.accentPlain)
            .accessibilityLabel(Text(verbatim: name))
        }
        else {
            content
        }
    }

    private var content: some View {
        HStack(spacing: ThemeSpace.group) {
            Text(name)
                .themeText(.mono)
                .cutLine(.fileName)
            if let damage {
                HStack(spacing: ThemeSpace.inline) {
                    StatusGlyph(.warning)
                    Text(damage)
                        .themeText(.fine)
                        .foregroundStyle(Theme.warning)
                }
                .fixedSize()
            }
            Spacer(minLength: 0)
            if let onPlay {
                playButton(onPlay)
            }
            Text(size)
                .themeText(.fine)
                .foregroundStyle(.tertiary)
                // A squeezed row truncates the name, never wraps the size.
                .fixedSize()
        }
        .contentShape(Rectangle())
    }

    private func playButton(_ action: @escaping () -> Void) -> some View {
        let label = localizedCoreString(
            isPlaying ? "ui.import.slots.stop" : "ui.import.slots.play"
        )
        return Button(action: action) {
            Image(systemName: isPlaying ? "stop.fill" : "play.fill")
                .themeIcon(.small)
                .foregroundStyle(
                    isPlaying
                        ? AnyShapeStyle(Theme.accent)
                        : AnyShapeStyle(.secondary)
                )
                .frame(
                    width: ThemeSize.hitTarget,
                    height: ThemeSize.hitTarget
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        .help(label)
        .accessibilityLabel(label)
    }
}
