import BaeKit
import SwiftUI

/// The library's page heading: the current browser mode, whose menu switches
/// modes.
struct LibraryModeHeading: View {
    /// 0 at full size, 1 collapsed into the strip, as the content scrolls.
    let collapseProgress: Double

    /// The chevron's size at full size and collapsed; its lift towards the cap
    /// height is a fraction of the heading's size.
    private static let chevronFull: CGFloat = 15
    private static let chevronCollapsed: CGFloat = 8
    private static let chevronLift: CGFloat = 0.23

    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                LibraryModeButtons(selected: uiStore.libraryBrowserMode) {
                    mode in
                    uiStore.setLibraryBrowserMode(mode)
                }
            }
        ) {
            let size = between(\.macOSSize)
            // The chevron is part of the heading's text, so it scales with it
            // and follows the word in either reading direction.
            (Text(uiStore.libraryBrowserMode.displayName)
                .font(.system(size: size, weight: ThemeText.display.weight))
                .tracking(between(\.tracking))
                + Text(verbatim: " ")
                + Text(Image(systemName: "chevron.down"))
                .font(.system(size: chevronSize, weight: .bold))
                // Centered on the heading's cap height.
                .baselineOffset(size * Self.chevronLift)
                .foregroundColor(Theme.textSecondary))
                .contentTransition(.interpolate)
        }
        .fixedSize()
        .accessibilityLabel(uiStore.libraryBrowserMode.displayName)
    }

    private var chevronSize: CGFloat {
        Self.chevronFull
            + (Self.chevronCollapsed - Self.chevronFull) * collapseProgress
    }

    /// A measure of the heading's text, from the display role at full size to
    /// the title role once collapsed.
    private func between(_ measure: KeyPath<ThemeText, CGFloat>) -> CGFloat {
        let full = ThemeText.display[keyPath: measure]
        let collapsed = ThemeText.title[keyPath: measure]
        return full + (collapsed - full) * collapseProgress
    }
}
