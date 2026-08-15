import BaeKit
import SwiftUI

/// The library's allowed range and preferred size drive the primary window.
/// The same window contracts to `WelcomeWindow.size` when the library closes.
enum MainWindow {
    static let sceneID = "main"
    static let minSize = CGSize(width: 900, height: 600)
    static let defaultSize = CGSize(width: 1350, height: 850)
}

/// Bootstrap is a fixed-size setup view inside the primary window.
enum WelcomeWindow {
    static let size = CGSize(width: 900, height: 600)
}

/// The main window's chrome around the shell — the minimum window size, the
/// themed background stretched to fill however large the window grows, and
/// the bottom line for a library load error. BaeApp renders it live; any
/// preview of shell screens should render the same composition.
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

/// Bootstrap content fixes the primary window to its current setup size.
/// Library-open errors remain inline in the chooser.
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

    /// Sample shell content standing in for the app's screens, so the chrome
    /// previews show the frame — minimum size, themed background, and the error
    /// line — around something rather than empty space.
    private struct ChromeSampleContent: View {
        var body: some View {
            VStack(spacing: 12) {
                Image(systemName: "music.note.list")
                    .font(.system(size: 48))
                    .foregroundStyle(.secondary)
                Text(verbatim: "Shell content")
                    .font(.title2)
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
