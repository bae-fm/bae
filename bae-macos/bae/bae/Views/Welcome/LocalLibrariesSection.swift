import BaeKit
import SwiftUI

/// Libraries already on this device — the primary "open" path. A row per
/// library opens it directly via `onOpen`, the same callback the restore and
/// create flows hand a ready library to.
struct LocalLibrariesSection: View {
    let libraries: [BridgeLibrary]
    let disabled: Bool
    let onOpen: (BridgeLibrary) -> Void

    var body: some View {
        VStack(spacing: 12) {
            Text(
                libraries.count == 1 ? "Your library" : "Your libraries"
            )
            .font(.headline)
            ForEach(libraries, id: \.id) { library in
                Button {
                    onOpen(library)
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(library.name)
                            .font(.body.bold())
                        // A library whose config won't load is shown, not hidden:
                        // losing it from the list is how it used to disappear.
                        if let error = library.error {
                            Text(error)
                                .font(.caption)
                                .foregroundStyle(.red)
                                .lineLimit(2)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                }
                .buttonStyle(.bordered)
                .disabled(disabled || library.error != nil)
            }
        }
        .frame(maxWidth: 320)
    }
}

#if DEBUG
    #Preview {
        LocalLibrariesSection(
            libraries: PreviewData.welcomeLibraries,
            disabled: false,
            onOpen: { _ in },
        )
        .padding()
    }
#endif
