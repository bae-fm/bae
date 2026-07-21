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
            // Our own chevron in place of the menu's built-in indicator,
            // sized against the heading so it scales with the collapse
            // instead of staying a fixed miniature. Concatenated into the
            // heading's own Text so it rides the text run — after the word,
            // and on its visual left in an RTL locale — instead of being a
            // separate view a menu style may reposition.
            (Text(uiStore.libraryBrowserMode.displayName)
                .font(
                    .system(size: 56 - 32 * collapseProgress, weight: .heavy)
                )
                .tracking(-1.4 + collapseProgress)
                + Text(verbatim: " ")
                + Text(Image(systemName: "chevron.down"))
                .font(
                    .system(size: 16 - 7 * collapseProgress, weight: .bold)
                )
                // Lifted off the baseline to sit optically centered on the
                // heading's cap height.
                .baselineOffset(14 - 9 * collapseProgress)
                .foregroundColor(.secondary))
                .contentTransition(.interpolate)
                // The heading is all caps-height glyphs, but the line box
                // still reserves descender space and the pull-down anchors
                // below it. Trim that dead strip from the frame so the menu
                // opens near the glyphs; scales with the font's descender.
                .padding(.bottom, -(12 - 8 * collapseProgress))
        }
        .menuStyle(.button)
        .buttonStyle(StaticLabelButtonStyle())
        .menuIndicator(.hidden)
        .fixedSize()
    }
}

/// Renders the label as-is in every state: no pressed-state dimming while
/// the heading's pull-down is open.
private struct StaticLabelButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
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
