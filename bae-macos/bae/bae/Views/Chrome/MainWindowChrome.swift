import BaeKit
import SwiftUI

/// The main window's sizes, in one place: the floor the chrome enforces and
/// the size a fresh install launches at (macOS restores the user's own frame
/// on later launches). The scene and the in-situ previews both read these, so
/// the canvas and the real window can't disagree.
enum MainWindow {
    static let minSize = CGSize(width: 900, height: 600)
    static let defaultSize = CGSize(width: 1350, height: 850)
}

/// The welcome window's fixed size. Bootstrap is a setup-assistant window,
/// not a resizable document window — the scene pins its resizability to this
/// content size, and the welcome previews render at exactly it.
enum WelcomeWindow {
    static let size = CGSize(width: 900, height: 600)
}

/// The main window's chrome around the shell — the minimum window size, the
/// themed background stretched to fill however large the window grows, and
/// the bottom line for a library load error. BaeApp renders it live; any
/// preview of shell screens should render the same composition.
struct MainWindowChrome<Content: View>: View {
    let loadError: String?
    @ViewBuilder
    let content: Content

    var body: some View {
        content
            .frame(
                minWidth: MainWindow.minSize.width,
                minHeight: MainWindow.minSize.height
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .windowBackground()
            .overlay(alignment: .bottom) {
                LoadErrorLine(loadError: loadError)
            }
    }
}

/// The welcome window's chrome around the bootstrap screens (welcome,
/// loading, unlock) — the fixed window size and the themed background. A failed
/// library open surfaces inline in the welcome chooser (its callout), not as a
/// bottom line here. BaeApp renders it live; the welcome previews render the
/// same composition, so the canvas shows a screen exactly as the window does
/// and the two cannot drift apart.
struct WelcomeWindowChrome<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        content
            .frame(
                width: WelcomeWindow.size.width,
                height: WelcomeWindow.size.height
            )
            .windowBackground()
    }
}

/// The bottom-of-window line reporting a failed library switch under the shell.
private struct LoadErrorLine: View {
    let loadError: String?

    var body: some View {
        if let loadError {
            Text(loadError)
                .foregroundStyle(.red)
                .padding()
        }
    }
}
