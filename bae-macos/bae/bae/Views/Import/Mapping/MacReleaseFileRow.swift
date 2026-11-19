import BaeKit
import SwiftUI

/// One file of a release: its name in mono and its size at the right, with
/// the damage line under them when the file is damaged. Given `onOpen`, the
/// name and size are one button that opens the file.
struct MacReleaseFileRow: View {
    let name: String
    let size: String
    var damage: String?
    var onOpen: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
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
            if let damage {
                Text(damage)
                    .themeText(.fine)
                    .foregroundStyle(Theme.warning)
            }
        }
    }

    private var content: some View {
        HStack(spacing: ThemeSpace.group) {
            Text(name)
                .themeText(.mono)
                .cutLine(.fileName)
            Spacer(minLength: 0)
            Text(size)
                .themeText(.fine)
                .foregroundStyle(.tertiary)
                // A squeezed row truncates the name, never wraps the size.
                .fixedSize()
        }
        .contentShape(Rectangle())
    }
}
