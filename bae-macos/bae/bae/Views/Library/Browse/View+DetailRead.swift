import SwiftUI

extension View {
    /// Keep a detail pane's read on `id` while this view shows: another id
    /// moves the one read, and no id, or the view going, ends it.
    func detailRead(
        id: String?,
        activate: @escaping @MainActor (String) -> Void,
        deactivate: @escaping @MainActor (String) -> Void
    ) -> some View {
        onChange(of: id, initial: true) { oldId, newId in
            if let newId {
                activate(newId)
            }
            else if let oldId {
                deactivate(oldId)
            }
        }
        .onDisappear {
            if let id { deactivate(id) }
        }
    }
}
