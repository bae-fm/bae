import BaeKit
import SwiftUI

/// Libraries on this device, each with Open (or Show in Finder when it won't
/// load) and Delete.
struct LocalLibrariesSection: View {
    let libraries: [BridgeLibrary]
    let disabled: Bool
    let canDeleteActiveLibrary: Bool
    let removingLibraryId: String?
    let onOpen: (BridgeLibrary) -> Void
    let onShowInFinder: (BridgeLibrary) -> Void
    let onRemove: (BridgeLibrary) -> Void

    var body: some View {
        VStack(spacing: 12) {
            WelcomeSectionHeader(title: "Your libraries")
            ForEach(libraries, id: \.id) { library in
                LibraryRow(
                    library: library,
                    disabled: disabled,
                    deleteDisabled: library.isActive
                        && !canDeleteActiveLibrary,
                    isRemoving: removingLibraryId == library.id,
                    onOpen: onOpen,
                    onShowInFinder: onShowInFinder,
                    onRemove: onRemove,
                )
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
    }
}

/// One library row; a library that won't load adds a warning glyph and tint.
private struct LibraryRow: View {
    let library: BridgeLibrary
    let disabled: Bool
    let deleteDisabled: Bool
    let isRemoving: Bool
    let onOpen: (BridgeLibrary) -> Void
    let onShowInFinder: (BridgeLibrary) -> Void
    let onRemove: (BridgeLibrary) -> Void

    var body: some View {
        HStack(spacing: 12) {
            if library.error != nil {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(Theme.warning)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(library.name)
                    .themeText(.rowTitle)
                if let error = library.error {
                    Text("Can't open: \(error)")
                        .themeText(.detail)
                        .foregroundStyle(Theme.warning)
                        .lineLimit(2)
                }
                else if let provider = library.cloudProvider {
                    Text(provider.displayName)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 12)
            if library.error != nil {
                Button("Show in Finder") { onShowInFinder(library) }
                    .buttonStyle(.bordered)
            }
            else {
                Button("Open") { onOpen(library) }
                    .buttonStyle(.bordered)
            }
            ZStack {
                Button("Delete", systemImage: "trash", role: .destructive) {
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
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            (library.error != nil ? Theme.warning : Color.secondary)
                .opacity(ThemeOpacity.tint)
        )
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
            onShowInFinder: { _ in },
            onRemove: { _ in },
        )
        .padding()
    }
#endif
