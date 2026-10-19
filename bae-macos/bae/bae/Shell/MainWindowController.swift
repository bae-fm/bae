import AppKit
import SwiftUI

/// The primary window.
@MainActor
final class MainWindowController: HostingWindowController {
    init(appDelegate: AppDelegate, onClose: @escaping () -> Void) {
        super
            .init(
                content: MainWindowRoot(appDelegate: appDelegate),
                contentSize: MainWindow.sizeOnMainScreen,
                // The content sets the window's size limits: fixed while
                // bootstrap shows, the library's minimum once it opens.
                sizing: [.minSize, .maxSize],
                // The window's title follows the content's navigation title.
                bridging: .title,
                onClose: onClose
            )
        guard let window else {
            preconditionFailure("the primary window controller has its window")
        }
        // The content reaches under the title bar, which shows only the
        // window buttons.
        window.styleMask.insert(.fullSizeContentView)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
    }
}
