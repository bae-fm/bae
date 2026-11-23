import AppKit
import BaeKit
import SwiftUI

extension View {
    /// Lets a titled window show its themed backing behind its native toolbar.
    /// Apply to the window's root view.
    func themedWindowTitleBar() -> some View {
        background(ThemedWindowTitleBar().allowsHitTesting(false))
    }
}

private struct ThemedWindowTitleBar: NSViewRepresentable {
    func makeNSView(context: Context) -> BackgroundView {
        BackgroundView(color: backgroundColor(in: context.environment))
    }

    func updateNSView(_ view: BackgroundView, context: Context) {
        view.color = backgroundColor(in: context.environment)
    }

    private func backgroundColor(in environment: EnvironmentValues) -> NSColor {
        NSColor(Theme.background.resolve(in: environment))
    }

    final class BackgroundView: NSView {
        var color: NSColor {
            didSet {
                if color != oldValue { window?.backgroundColor = color }
            }
        }

        init(color: NSColor) {
            self.color = color
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) is not supported")
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let window else { return }
            // The window backing shows behind the title bar. Keep the content
            // view below the toolbar so AppKit still sizes it separately.
            window.backgroundColor = color
            window.titlebarAppearsTransparent = true
        }
    }
}
