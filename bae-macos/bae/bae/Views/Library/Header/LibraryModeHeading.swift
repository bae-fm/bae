import BaeKit
import SwiftUI

/// The library's page heading: the current browser mode, whose menu switches
/// modes.
struct LibraryModeHeading: View {
    /// 0 at full size, 1 collapsed into the strip, as the content scrolls.
    let collapseProgress: Double

    /// The chevron's size, its lift towards the cap height, and the descender
    /// space trimmed under the heading, each a fraction of the heading's size.
    private static let chevronScale: CGFloat = 1 / 3
    private static let chevronLift: CGFloat = 0.23
    private static let descenderTrim: CGFloat = 0.22

    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Menu {
            LibraryModeButtons(selected: uiStore.libraryBrowserMode) { mode in
                uiStore.setLibraryBrowserMode(mode)
            }
        } label: {
            let size = between(\.macOSSize)
            // The chevron is part of the heading's text, so it scales with it
            // and follows the word in either reading direction.
            (Text(uiStore.libraryBrowserMode.displayName)
                .font(.system(size: size, weight: ThemeText.display.weight))
                .tracking(between(\.tracking))
                + Text(verbatim: " ")
                + Text(Image(systemName: "chevron.down"))
                .font(.system(size: size * Self.chevronScale, weight: .bold))
                // Centered on the heading's cap height.
                .baselineOffset(size * Self.chevronLift)
                .foregroundColor(.secondary))
                .contentTransition(.interpolate)
                // Trims the unused descender space so the menu opens near the
                // text.
                .padding(.bottom, -size * Self.descenderTrim)
        }
        .menuStyle(.button)
        .buttonStyle(StaticLabelButtonStyle())
        .menuIndicator(.hidden)
        .fixedSize()
    }

    /// A measure of the heading's text, from the display role at full size to
    /// the title role once collapsed.
    private func between(_ measure: KeyPath<ThemeText, CGFloat>) -> CGFloat {
        let full = ThemeText.display[keyPath: measure]
        let collapsed = ThemeText.title[keyPath: measure]
        return full + (collapsed - full) * collapseProgress
    }
}

/// Draws the label unchanged while the menu is open.
private struct StaticLabelButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
    }
}

#if DEBUG
    #Preview {
        VStack(alignment: .leading, spacing: ThemeSpace.section) {
            LibraryModeHeading(collapseProgress: 0)
            LibraryModeHeading(collapseProgress: 0.5)
            LibraryModeHeading(collapseProgress: 1)
        }
        .padding()
        .environment(UiStore())
    }
#endif
