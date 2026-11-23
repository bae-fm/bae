import Observation

/// The queued entry under the pointer, shared by Up next and Playing from.
@MainActor
@Observable
final class QueueRowHover {
    private(set) var entryId: String?

    init(entryId: String? = nil) {
        self.entryId = entryId
    }

    func set(_ hovering: Bool, entryId: String) {
        if hovering {
            self.entryId = entryId
        }
        else if self.entryId == entryId {
            // One row's exit can arrive after another row's enter.
            self.entryId = nil
        }
    }

    func clear() {
        entryId = nil
    }
}
