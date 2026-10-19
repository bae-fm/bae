import AppKit
import Testing

@testable import bae

/// The size the primary window takes as a library opens, on displays of
/// different sizes.
@MainActor
@Suite("MainWindow size")
struct MainWindowSizeTests {
    /// The window around a library opened at `size`, title bar included.
    private func windowFrame(around size: CGSize) -> CGSize {
        NSWindow.frameRect(
            forContentRect: CGRect(origin: .zero, size: size),
            styleMask: .titled
        )
        .size
    }

    @Test("a library opens at the default size on a display with room for it")
    func roomyDisplay() {
        #expect(
            MainWindow.size(fitting: CGSize(width: 2560, height: 1410))
                == MainWindow.defaultSize
        )
    }

    @Test("a library opens inside a display smaller than the default size")
    func smallDisplay() {
        // A 1024x768 screen, less its menu bar and Dock.
        let visibleArea = CGSize(width: 1024, height: 675)

        let frame = windowFrame(around: MainWindow.size(fitting: visibleArea))

        #expect(frame.width == visibleArea.width)
        #expect(frame.height == visibleArea.height)
    }

    @Test("a library never opens under the minimum size")
    func displaySmallerThanTheMinimum() {
        #expect(
            MainWindow.size(fitting: CGSize(width: 800, height: 500))
                == MainWindow.minSize
        )
    }
}
