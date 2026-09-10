import BaeKit

/// A work group's id is core's: the parent work's id, or the lone work's id
/// for an ungrouped work. The composer pane's works list iterates groups by it.
extension BridgeComposerWorkGroup: Identifiable {}
