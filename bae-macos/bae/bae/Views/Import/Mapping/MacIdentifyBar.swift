import BaeKit
import SwiftUI

/// The bar of identify entries: the Identify label, Automatic and Search,
/// the Metadata menu where the caller offers its commands, and the caller's
/// own trailing controls at the far end.
struct MacIdentifyBar<Trailing: View>: View {
    /// The space between the identify group and the trailing controls.
    let spacing: CGFloat
    /// Whether a read or save in flight disables the identify controls.
    let identifyDisabled: Bool
    let onAutomatic: () -> Void
    let onSearch: () -> Void
    /// Runs the command the Metadata menu picks; `nil` draws no menu.
    let onMetadataCommand: ((ImportDraftCommand) -> Void)?
    @ViewBuilder
    let trailing: () -> Trailing

    var body: some View {
        HStack(alignment: .center, spacing: spacing) {
            HStack(spacing: ThemeSpace.related) {
                Text("Identify")
                    .themeText(.strong)
                    .foregroundStyle(.secondary)
                Button("Automatic") { onAutomatic() }
                    .buttonStyle(.accentBordered)
                Button("Search") { onSearch() }
                    .buttonStyle(.accentBordered)
                if let onMetadataCommand {
                    metadataMenu(onMetadataCommand)
                }
            }
            .disabled(identifyDisabled)
            Spacer(minLength: ThemeSpace.group)
            trailing()
        }
    }

    /// Commands that replace the draft's metadata.
    private func metadataMenu(
        _ run: @escaping (ImportDraftCommand) -> Void
    ) -> some View {
        MacButton(
            kind: .secondary,
            menu: {
                for command in ImportDraftCommand.allCases {
                    MacMenuItem(verbatim: command.title) {
                        run(command)
                    }
                }
            }
        ) {
            Image(systemName: "ellipsis")
        }
        .accessibilityLabel(Text("Metadata"))
        .fixedSize()
    }
}
