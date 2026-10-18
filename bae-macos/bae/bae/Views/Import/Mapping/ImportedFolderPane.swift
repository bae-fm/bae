import BaeKit
import SwiftUI

/// Every matching copy is listed; one copy owns the active metadata editor.
struct ImportedFolderPane<Content: View>: View {
    let releases: [BridgeImportedRelease]
    @ViewBuilder
    let content: (BridgeImportedRelease) -> Content

    @State
    private var selectedReleaseId: String?

    private var selectedRelease: BridgeImportedRelease {
        guard let first = releases.first else {
            preconditionFailure("a Done folder has at least one library copy")
        }
        return releases.first { $0.releaseId == selectedReleaseId } ?? first
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                Text(verbatim: coreString("ui.import.probably_in_library"))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, ThemeSpace.section)
                Picker(
                    "Release",
                    selection: Binding(
                        get: { selectedRelease.releaseId },
                        set: { selectedReleaseId = $0 }
                    )
                ) {
                    ForEach(
                        Array(releases.enumerated()),
                        id: \.element.releaseId
                    ) { index, release in
                        Text(
                            verbatim: coreString(
                                "core.release.numbered",
                                index + 1
                            )
                        )
                        .tag(release.releaseId)
                    }
                }
                .padding(.horizontal, ThemeSpace.section)
                content(selectedRelease)
                    .id(selectedRelease.releaseId)
            }
        }
    }
}
