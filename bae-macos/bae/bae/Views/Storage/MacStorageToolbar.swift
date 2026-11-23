import AppKit
import BaeKit
import SwiftUI

/// The Storage Manager window's toolbar: the All / Local / Cloud control,
/// the Filter field and the Inspector button. `off` draws each in its own off
/// state, for the window with no library open, where none of them does
/// anything.
struct MacStorageToolbar: ViewModifier {
    let filter: Binding<BridgeStorageFilter>
    let search: Binding<String>
    let inspectorPresented: Binding<Bool>
    var off = false

    func body(content: Content) -> some View {
        content
            .background(StorageToolbarBackground().allowsHitTesting(false))
            .toolbar {
                ToolbarItem(placement: .principal) {
                    MacSegmentedControl(
                        selectedIndex: Binding(
                            get: { filter.wrappedValue.segmentIndex },
                            set: {
                                filter.wrappedValue = .init(segmentIndex: $0)
                            }
                        ),
                        segments: [
                            .init(
                                label: String(localized: "All"),
                                systemImage: nil
                            ),
                            .init(
                                label: String(localized: "Local"),
                                systemImage: nil
                            ),
                            .init(
                                label: String(localized: "Cloud"),
                                systemImage: nil
                            ),
                        ],
                        off: off
                    )
                }
                .unboxed()
                ToolbarItem(placement: .primaryAction) {
                    StorageFilterField(text: search)
                        .disabled(off)
                }
                .unboxed()
                ToolbarItem(placement: .primaryAction) {
                    MacIconButton(
                        symbol: "sidebar.trailing",
                        label: "Inspector",
                        active: inspectorPresented.wrappedValue,
                        iconSize: .medium
                    ) {
                        inspectorPresented.wrappedValue.toggle()
                    }
                    .frame(width: 28, height: 28)
                    .fixedSize()
                    .disabled(off)
                }
                .unboxed()
            }
    }
}

/// Shows the themed window backing through the native title bar.
private struct StorageToolbarBackground: NSViewRepresentable {
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
                if color != oldValue { applyBackground() }
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
            applyBackground()
        }

        private func applyBackground() {
            guard let window, window.styleMask.contains(.titled) else { return }
            // The window backing shows behind the title bar. Keep the content
            // view below the toolbar so AppKit still sizes it separately.
            window.backgroundColor = color
            window.titlebarAppearsTransparent = true
        }
    }
}

/// The Filter field in the toolbar; while the window is off it is dimmed
/// and takes no text.
private struct StorageFilterField: View {
    @Binding
    var text: String
    @FocusState
    private var focused: Bool

    var body: some View {
        MacSearchField(
            kind: .filter,
            text: $text,
            prompt: "Filter",
            focused: $focused,
            onEscape: { focused = false }
        )
    }
}

extension BridgeStorageFilter {
    /// The place of this filter among the All / Local / Cloud segments.
    fileprivate var segmentIndex: Int {
        switch self {
        case .all: 0
        case .local: 1
        case .remote: 2
        }
    }

    /// The filter of the segment at `segmentIndex`.
    fileprivate init(segmentIndex: Int) {
        switch segmentIndex {
        case 0: self = .all
        case 1: self = .local
        case 2: self = .remote
        default: preconditionFailure("the control has three segments")
        }
    }
}

extension ToolbarContent {
    /// Draws the item with no glass capsule behind it, as the design draws a
    /// toolbar's icon button: the glyph's own 28 point button. macOS before 26
    /// draws no capsule.
    @ToolbarContentBuilder
    fileprivate func unboxed() -> some ToolbarContent {
        if #available(macOS 26, *) {
            self.sharedBackgroundVisibility(.hidden)
        }
        else {
            self
        }
    }
}
