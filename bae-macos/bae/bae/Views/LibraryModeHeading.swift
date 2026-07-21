import SwiftUI

/// The library screen's page heading, doubling as the browser-mode switcher.
/// The big bold label *is* the current mode ("Albums" / "Composers") and its
/// dropdown switches modes, so the switcher stays in one fixed spot as the
/// content below swaps. Reads and writes mode through the environment `UiStore`.
struct LibraryModeHeading: View {
    /// 0 = full page-heading size, 1 = compact strip; intermediate values
    /// scrub between them as the content scrolls (`HeaderCollapse.progress`).
    let collapseProgress: Double

    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Menu {
            LibraryModeButtons(uiStore: uiStore) { mode in
                uiStore.setLibraryBrowserMode(mode)
            }
        } label: {
            Text(uiStore.libraryBrowserMode.displayName)
                .font(
                    .system(size: 56 - 34 * collapseProgress, weight: .heavy)
                )
                .tracking(-1.4 + collapseProgress)
                .contentTransition(.interpolate)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }
}

#if DEBUG
    #Preview {
        VStack(alignment: .leading, spacing: 20) {
            LibraryModeHeading(collapseProgress: 0)
            LibraryModeHeading(collapseProgress: 0.5)
            LibraryModeHeading(collapseProgress: 1)
        }
        .padding()
        .environment(UiStore())
    }
#endif
