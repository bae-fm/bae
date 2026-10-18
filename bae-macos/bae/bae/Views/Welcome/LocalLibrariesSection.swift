import BaeKit
import SwiftUI

/// Libraries on this device, each with Open and Remove.
struct LocalLibrariesSection: View {
    let libraries: [BridgeLibrary]
    let disabled: Bool
    let canDeleteActiveLibrary: Bool
    let removingLibraryId: String?
    let onOpen: (BridgeLibrary) -> Void
    let onRemove: (BridgeLibrary) -> Void

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            WelcomeSectionHeader(title: "Your libraries")
            ForEach(libraries, id: \.id) { library in
                LibraryRow(
                    library: library,
                    disabled: disabled,
                    deleteDisabled: library.isActive
                        && !canDeleteActiveLibrary,
                    isRemoving: removingLibraryId == library.id,
                    onOpen: onOpen,
                    onRemove: onRemove,
                )
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
    }
}

/// One library row: its name, its provider when it has a cloud home, Open and
/// Remove.
private struct LibraryRow: View {
    let library: BridgeLibrary
    let disabled: Bool
    let deleteDisabled: Bool
    let isRemoving: Bool
    let onOpen: (BridgeLibrary) -> Void
    let onRemove: (BridgeLibrary) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(library.name)
                    .themeText(.rowTitle)
                // Every row keeps the detail line so rows share one height.
                Text(library.storage?.displayName ?? "")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: ThemeSpace.group)
            Button("Open") { onOpen(library) }
                .buttonStyle(.bordered)
            ZStack {
                Button("Remove", systemImage: "trash", role: .destructive) {
                    onRemove(library)
                }
                .buttonStyle(.bordered)
                .disabled(deleteDisabled)
                .opacity(isRemoving ? 0 : 1)
                .allowsHitTesting(!isRemoving)
                ProgressView()
                    .controlSize(.small)
                    .opacity(isRemoving ? 1 : 0)
            }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.secondary.opacity(ThemeOpacity.tint))
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
        .disabled(disabled)
    }
}

#if DEBUG
    #Preview {
        LocalLibrariesSection(
            libraries: PreviewData.welcomeLibraries,
            disabled: false,
            canDeleteActiveLibrary: true,
            removingLibraryId: nil,
            onOpen: { _ in },
            onRemove: { _ in },
        )
        .padding()
    }
#endif
