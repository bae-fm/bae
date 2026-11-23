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
