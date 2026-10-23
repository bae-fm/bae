import BaeKit
import SwiftUI

/// The primary window's sizes while a library is open; it contracts to
/// `WelcomeWindow.size` when the library closes.
enum MainWindow {
    static let minSize = CGSize(width: 900, height: 600)
    /// The size a library opens at on a display with room for it.
    static let defaultSize = CGSize(width: 1350, height: 850)

    /// The size a library opens at on a display whose area beside the menu
    /// bar and Dock is `visibleArea`: `defaultSize`, cut down so the window
    /// and its title bar fit in that area, and never under `minSize`.
    @MainActor
    static func size(fitting visibleArea: CGSize) -> CGSize {
        let titleBarHeight =
            NSWindow.frameRect(forContentRect: .zero, styleMask: .titled)
            .height
        return CGSize(
            width: max(
                minSize.width,
                min(defaultSize.width, visibleArea.width)
            ),
            height: max(
                minSize.height,
                min(defaultSize.height, visibleArea.height - titleBarHeight)
            )
        )
    }

    /// The size a library opens at on the screen of the window with the
    /// keyboard focus, which is where the primary window opens a library.
    @MainActor
    static var sizeOnMainScreen: CGSize {
        guard let screen = NSScreen.main else {
            preconditionFailure("the primary window opens with no screen")
        }
        return size(fitting: screen.visibleFrame.size)
    }
}

/// Bootstrap is a fixed-size setup view inside the primary window.
enum WelcomeWindow {
    static let size = CGSize(width: 900, height: 600)
}

/// The main window's chrome around the shell: its size limits, the themed
/// background, and the bottom notices for what launch found wrong and for a
/// library load error, each its own.
struct MainWindowChrome<Content: View>: View {
    var launchProblems = LaunchProblems.none
    let loadError: LoadError?
    @ViewBuilder
    let content: Content

    var body: some View {
        MainWindowFrame {
            content
        }
        .windowBackground()
        .overlay(alignment: .bottom) {
            VStack(spacing: ThemeSpace.related) {
                LaunchProblemNotices(launchProblems)
                LoadErrorLine(loadError: loadError)
            }
            .padding()
        }
    }
}

/// The library window's size limits: never under `MainWindow.minSize`, and
/// as large as the person makes it. The limits never depend on the content,
/// so the window hosting the shell, which measures its content's minimum,
/// maximum and ideal sizes whenever the content changes, measures only this
/// layout and never the shell, whatever in the shell changed: during
/// playback, the position it shows changes several times a second.
private struct MainWindowFrame: Layout {
    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews _: Subviews,
        cache _: inout Void
    ) -> CGSize {
        CGSize(
            width: max(
                proposal.width ?? MainWindow.minSize.width,
                MainWindow.minSize.width
            ),
            height: max(
                proposal.height ?? MainWindow.minSize.height,
                MainWindow.minSize.height
            )
        )
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal _: ProposedViewSize,
        subviews: Subviews,
        cache _: inout Void
    ) {
        for subview in subviews {
            subview.place(
                at: CGPoint(x: bounds.midX, y: bounds.midY),
                anchor: .center,
                proposal: ProposedViewSize(bounds.size)
            )
        }
    }
}

/// Fixes the primary window to the setup size while bootstrap shows.
struct WelcomeWindowChrome<Content: View>: View {
    var size = WelcomeWindow.size
    @ViewBuilder
    let content: Content

    var body: some View {
        content
            .frame(
                width: size.width,
                height: size.height
            )
            .windowBackground()
    }
}

/// The bottom-of-window notice reporting a load-time failure under the
/// shell: a failed library switch, close or removal.
private struct LoadErrorLine: View {
    let loadError: LoadError?

    var body: some View {
        if let loadError {
            LoadErrorNotice(loadError: loadError)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// Stand-in shell content for the chrome to frame.
    private struct ChromeSampleContent: View {
        var body: some View {
            Text(verbatim: "Shell content")
                .themeText(.title)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    #Preview("Main window") {
        MainWindowChrome(loadError: nil) {
            ChromeSampleContent()
        }
    }

    #Preview("Main window — load error") {
        MainWindowChrome(
            loadError: LoadError(.shutdown, PreviewData.displayErrorWithDetail)
        ) {
            ChromeSampleContent()
        }
    }

    #Preview("Welcome window") {
        WelcomeWindowChrome {
            ChromeSampleContent()
        }
    }
#endif
