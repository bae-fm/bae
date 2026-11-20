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
    /// Runs the command the Metadata menu picks.
    let onMetadataCommand: (ReleaseMetadataCommand) -> Void
    @ViewBuilder
    let trailing: () -> Trailing

    var body: some View {
        // The trailing controls wrap onto a second line, at the right, when
        // the pane has no room for them beside the identify group.
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .center, spacing: spacing) {
                identifyGroup
                Spacer(minLength: ThemeSpace.group)
                trailing()
            }
            VStack(alignment: .trailing, spacing: ThemeSpace.related) {
                identifyGroup
                    .frame(maxWidth: .infinity, alignment: .leading)
                trailing()
            }
        }
    }

    private var identifyGroup: some View {
        HStack(spacing: ThemeSpace.related) {
            Text("Identify")
                .themeText(.strong)
                .foregroundStyle(Theme.macTextSecondary)
            Button("Automatic") { onAutomatic() }
                .buttonStyle(.accentBordered)
            Button("Search") { onSearch() }
                .buttonStyle(.accentBordered)
            metadataMenu(onMetadataCommand)
        }
        .disabled(identifyDisabled)
    }

    /// Commands that replace the draft's metadata.
    private func metadataMenu(
        _ run: @escaping (ReleaseMetadataCommand) -> Void
    ) -> some View {
        MacButton(
            kind: .secondary,
            menu: {
                for command in ReleaseMetadataCommand.allCases {
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
