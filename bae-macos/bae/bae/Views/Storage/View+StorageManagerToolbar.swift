import BaeKit
import SwiftUI

extension View {
    /// The Storage Manager window's toolbar: the All / Local / Cloud
    /// control, the Filter field and the Inspector button. `off` draws each
    /// in its own off state, for the window with no library open, where none
    /// of them does anything.
    func storageManagerToolbar(
        filter: Binding<BridgeStorageFilter>,
        search: Binding<String>,
        inspectorPresented: Binding<Bool>,
        off: Bool = false
    ) -> some View {
        searchable(
            text: search,
            placement: .toolbar,
            prompt: Text("Filter")
        )
        .disabled(off)
        .toolbar {
            ToolbarItem(placement: .principal) {
                MacSegmentedControl(
                    selectedIndex: Binding(
                        get: {
                            StorageFilterSegment.order.firstIndex(
                                of: filter.wrappedValue
                            ) ?? 0
                        },
                        set: {
                            filter.wrappedValue = StorageFilterSegment.order[$0]
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
            ToolbarItem(placement: .primaryAction) {
                Button {
                    inspectorPresented.wrappedValue.toggle()
                } label: {
                    Image(systemName: "sidebar.trailing")
                        .themeIcon(.small)
                        .frame(
                            width: ThemeSize.hitTarget,
                            height: ThemeSize.hitTarget
                        )
                }
                .buttonStyle(
                    IconHoverButtonStyle(isOn: inspectorPresented.wrappedValue)
                )
                .accentFocusRing(cornerRadius: ThemeRadius.control)
                .help("Inspector")
                .accessibilityLabel(Text("Inspector"))
                .disabled(off)
            }
        }
    }
}

/// The segments' order in the All / Local / Cloud control.
private enum StorageFilterSegment {
    static let order: [BridgeStorageFilter] = [.all, .local, .remote]
}
