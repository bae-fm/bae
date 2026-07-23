import BaeKit
import SwiftUI

/// Libraries already on this device — the primary "open" path. Each healthy
/// row shows the library's name over its cloud provider and an Open button; a
/// library whose config won't load shows as an amber warning with Show in
/// Finder so the user can inspect its folder rather than hitting a dead end.
struct LocalLibrariesSection: View {
    let libraries: [BridgeLibrary]
    let disabled: Bool
    let onOpen: (BridgeLibrary) -> Void
    let onShowInFinder: (BridgeLibrary) -> Void

    var body: some View {
        VStack(spacing: 12) {
            WelcomeSectionHeader(title: "Your libraries")
            ForEach(libraries, id: \.id) { library in
                LibraryRow(
                    library: library,
                    disabled: disabled,
                    onOpen: onOpen,
                    onShowInFinder: onShowInFinder,
                )
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
    }
}

/// One library row. Healthy and broken share the same shape — name line,
/// caption line, trailing button — so the column keeps a steady rhythm; only
/// the broken row adds a leading warning glyph and swaps its tint. The branch
/// is on the library's immutable `error`, not on any toggling `@State`, so it
/// never re-measures at runtime (unlike the layout-stability opacity pattern the
/// keychain rows need for their in-flight controls).
private struct LibraryRow: View {
    let library: BridgeLibrary
    let disabled: Bool
    let onOpen: (BridgeLibrary) -> Void
    let onShowInFinder: (BridgeLibrary) -> Void

    var body: some View {
        HStack(spacing: 12) {
            if library.error != nil {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(library.name)
                    .font(.body.bold())
                if let error = library.error {
                    Text("Can't open — \(error)")
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .lineLimit(2)
                }
                else if let provider = library.cloudProvider {
                    Text(provider.displayName)
                        .font(.caption)
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
                    .disabled(disabled)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            (library.error != nil ? Color.orange : Color.secondary)
                .opacity(0.1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }
}

#if DEBUG
    #Preview {
        LocalLibrariesSection(
            libraries: PreviewData.welcomeLibraries,
            disabled: false,
            onOpen: { _ in },
            onShowInFinder: { _ in },
        )
        .padding()
    }
#endif
