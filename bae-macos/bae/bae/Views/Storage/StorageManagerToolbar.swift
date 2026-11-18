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
                Picker("Filter", selection: filter) {
                    Text("All").tag(BridgeStorageFilter.all)
                    Text("Local").tag(BridgeStorageFilter.local)
                    Text("Cloud").tag(BridgeStorageFilter.remote)
                }
                .pickerStyle(.segmented)
                .accentFocusRing(cornerRadius: .infinity)
                .labelsHidden()
                .disabled(off)
            }
            ToolbarItem(placement: .primaryAction) {
                Toggle(isOn: inspectorPresented) {
                    Label("Inspector", systemImage: "sidebar.trailing")
                        .labelStyle(.iconOnly)
                }
                .toggleStyle(.button)
                .foregroundStyle(
                    inspectorPresented.wrappedValue
                        ? Theme.accent : Color.secondary
                )
                .accentFocusRing(cornerRadius: .infinity)
                .disabled(off)
            }
        }
    }
}
