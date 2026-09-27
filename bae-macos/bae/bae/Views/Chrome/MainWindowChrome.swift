import BaeKit
import SwiftUI

/// The primary window's sizes while a library is open; it contracts to
/// `WelcomeWindow.size` when the library closes.
enum MainWindow {
    static let sceneID = "main"
    static let minSize = CGSize(width: 900, height: 600)
    static let defaultSize = CGSize(width: 1350, height: 850)
}

/// Bootstrap is a fixed-size setup view inside the primary window.
enum WelcomeWindow {
    static let size = CGSize(width: 900, height: 600)
}

/// The main window's chrome around the shell: its minimum size, the themed
/// background, and the bottom line for a library load error.
struct MainWindowChrome<Content: View>: View {
    let loadError: DisplayError?
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

/// The bottom-of-window line reporting a failed library switch under the shell.
private struct LoadErrorLine: View {
    let loadError: DisplayError?

    var body: some View {
        if let loadError {
            ErrorDetailDisclosure(error: loadError)
                .padding()
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// Stand-in shell content for the chrome to frame.
    private struct ChromeSampleContent: View {
        var body: some View {
            VStack(spacing: ThemeSpace.group) {
                Image(systemName: "music.note.list")
                    .themeIcon(.hero)
                    .foregroundStyle(.secondary)
                Text(verbatim: "Shell content")
                    .themeText(.title)
                    .foregroundStyle(.secondary)
            }
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
            loadError: PreviewData.displayErrorWithDetail
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
