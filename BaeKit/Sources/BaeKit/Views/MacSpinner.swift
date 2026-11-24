import SwiftUI

/// A wait with no length and no steps of its own, such as a list still
/// filling: the small spinner, with the words that say what is happening
/// beside it when there are any. A wait whose length is known, or a step of
/// a flow, draws a bar instead.
public struct MacSpinner: View {
    private let words: Text?
    private let size: CGFloat?

    /// The spinner alone, where what it waits on is said elsewhere.
    public init() {
        words = nil
        size = nil
    }

    /// The spinner alone, 11 points across as a glyph at the end of a menu
    /// line is.
    public static var glyph: MacSpinner {
        sized(11)
    }

    /// The spinner alone, `size` points across.
    public static func sized(_ size: CGFloat) -> MacSpinner {
        MacSpinner(size: size)
    }

    private init(size: CGFloat) {
        words = nil
        self.size = size
    }

    /// The spinner, with `words` beside it.
    public init(_ words: Text) {
        self.words = words
        size = nil
    }

    /// The spinner `size` points across, with `words` beside it.
    public init(_ words: Text, size: CGFloat) {
        self.words = words
        self.size = size
    }

    /// The spinner, with the words `key` names beside it.
    public init(_ key: LocalizedStringKey) {
        words = Text(key)
        size = nil
    }

    /// The system's small spinner is 16 points across; a smaller one draws
    /// it scaled into its size.
    @ViewBuilder
    private var sized: some View {
        if let size {
            SystemSpinner()
                .scaleEffect(size / 16)
                .frame(width: size, height: size)
        }
        else {
            SystemSpinner()
        }
    }

    /// The spinner, named by the words beside it when it has them.
    private var spinner: some View {
        sized
            .accessibilityLabel(
                words ?? Text("Loading", tableName: "Controls", bundle: .module)
            )
    }

    public var body: some View {
        HStack(spacing: ThemeSpace.related) {
            spinner
            if let words {
                words
                    .themeText(.detail)
                    .foregroundStyle(Theme.textSecondary)
            }
        }
    }
}

#if canImport(AppKit)
    extension MacSpinner {
        final class NativeIndicator: NSProgressIndicator {
            /// Whether AppKit was last told to animate. AppKit has no way to
            /// ask.
            private(set) var animating = false

            override func accessibilityValue() -> NSNumber? { nil }

            /// AppKit stops a spinner that leaves its window and ignores a
            /// start sent before it has one, so the animation starts each
            /// time a window takes the view and stops when it leaves.
            override func viewDidMoveToWindow() {
                super.viewDidMoveToWindow()
                syncAnimation()
            }

            override func viewDidUnhide() {
                super.viewDidUnhide()
                syncAnimation()
            }

            /// A child window is put under its parent after its content is
            /// in it, so the window that holds the view can change what it
            /// holds still before the view's first draw.
            override func viewWillDraw() {
                super.viewWillDraw()
                syncAnimation()
            }

            /// Turns while a window holds the view, unless that window holds
            /// spinners still, as the shot renderer's does, so a capture
            /// draws the same frame each time. The still spinner stays drawn.
            func syncAnimation() {
                let turns = window.map { !$0.holdsSpinnersStill } ?? false
                guard turns != animating else { return }
                animating = turns
                if turns {
                    startAnimation(nil)
                }
                else {
                    stopAnimation(nil)
                }
            }
        }
    }

    nonisolated(unsafe) private var holdsSpinnersStillKey: UInt8 = 0

    extension NSWindow {
        /// Whether the spinners in this window hold still. The shot renderer
        /// sets it on its windows; the app never does. A view the app hosts
        /// apart from the window's SwiftUI tree, such as a table cell, is in
        /// the window all the same, which an environment value would not
        /// reach. The menus, popovers and alerts of a window are its child
        /// windows, and a sheet's window is its parent's sheet: each holds
        /// still as the window it belongs to does, unless it is set itself.
        /// Setting it stops or starts the spinners already in the window and
        /// in those windows.
        public var holdsSpinnersStill: Bool {
            get {
                (objc_getAssociatedObject(self, &holdsSpinnersStillKey)
                    as? Bool)
                    ?? (sheetParent ?? parent)?.holdsSpinnersStill ?? false
            }
            set {
                objc_setAssociatedObject(
                    self,
                    &holdsSpinnersStillKey,
                    newValue,
                    .OBJC_ASSOCIATION_RETAIN_NONATOMIC
                )
                syncSpinners()
            }
        }

        private func syncSpinners() {
            (contentView?.superview ?? contentView)?.syncSpinners()
            for child in childWindows ?? [] {
                child.syncSpinners()
            }
            attachedSheet?.syncSpinners()
        }
    }

    extension NSView {
        fileprivate func syncSpinners() {
            (self as? MacSpinner.NativeIndicator)?.syncAnimation()
            for view in subviews {
                view.syncSpinners()
            }
        }
    }

    /// macOS's own small spinner, which AppKit draws. A wait of no known
    /// length has no amount done, so VoiceOver reads no value: SwiftUI's
    /// indeterminate progress view reports 0, and AppKit's spinner 1.
    private struct SystemSpinner: NSViewRepresentable {
        func makeNSView(context: Context) -> MacSpinner.NativeIndicator {
            let indicator = MacSpinner.NativeIndicator()
            indicator.style = .spinning
            indicator.isIndeterminate = true
            indicator.controlSize = .small
            indicator.isDisplayedWhenStopped = true
            return indicator
        }

        func updateNSView(
            _ indicator: MacSpinner.NativeIndicator,
            context: Context
        ) {}

        func sizeThatFits(
            _ proposal: ProposedViewSize,
            nsView indicator: MacSpinner.NativeIndicator,
            context: Context
        ) -> CGSize? {
            indicator.intrinsicContentSize
        }
    }
#else
    private struct SystemSpinner: View {
        var body: some View {
            ProgressView().controlSize(.small)
        }
    }
#endif
