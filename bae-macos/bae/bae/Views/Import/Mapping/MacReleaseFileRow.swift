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
                        .themeText(.detail)
                        .foregroundStyle(Theme.warning)
                }
                .fixedSize()
            }
            Spacer(minLength: 0)
            if let onPlay {
                MacAuditionButton(isPlaying: isPlaying, action: onPlay)
            }
            Text(size)
                .themeText(.cell)
                .foregroundStyle(Theme.textTertiary)
                // A squeezed row truncates the name, never wraps the size.
                .fixedSize()
        }
        .padding(.horizontal, ThemeSpace.line)
        .padding(.vertical, ThemeSpace.related)
        .contentShape(Rectangle())
    }
}
