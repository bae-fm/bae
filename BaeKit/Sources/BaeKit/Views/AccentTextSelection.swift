#if os(macOS)
    import AppKit
    import SwiftUI

    /// Colours the text selected in the window's text fields, and their
    /// caret, in the accent the person chose. macOS draws both in the Mac's
    /// accent, which bae never shows. Every field in a window edits in the
    /// window's one shared field editor (an `NSTextView`), so setting its
    /// documented selection attributes colours them all.
    struct AccentTextSelection: NSViewRepresentable {
        let accent: AccentChoice

        func makeNSView(context _: Context) -> SelectionView {
            SelectionView(accent: accent)
        }

        func updateNSView(_ view: SelectionView, context _: Context) {
            view.accent = accent
        }

        final class SelectionView: NSView {
            var accent: AccentChoice {
                didSet { apply() }
            }

            init(accent: AccentChoice) {
                self.accent = accent
                super.init(frame: .zero)
            }

            @available(*, unavailable)
            required init?(coder _: NSCoder) {
                fatalError("init(coder:) has not been implemented")
            }

            override func viewDidMoveToWindow() {
                super.viewDidMoveToWindow()
                apply()
            }

            private func apply() {
                guard
                    let editor = window?.fieldEditor(true, for: nil)
                        as? NSTextView
                else { return }
                let color = accent.appKitColor
                editor.selectedTextAttributes = [
                    .backgroundColor: color.withAlphaComponent(
                        ThemeOpacity.tintStrong
                    )
                ]
                editor.insertionPointColor = color
            }
        }
    }
#endif
