import Foundation

/// The bulk-action menu a grid card presents: how many albums it targets and the
/// four actions, each already bound to those targets. Built by the grid from
/// `AlbumGridSelection.orderedTargets`; both the SwiftUI context menu and the
/// AppKit ellipsis menu render from this one definition. Labels switch to the
/// plural form (carrying the count) when more than one album is targeted.
struct AlbumCardMenu {
    let targetCount: Int
    let onPlay: () -> Void
    let onAddToQueue: () -> Void
    let onAddNext: () -> Void
    let onPin: () -> Void

    var playLabel: String {
        targetCount > 1
            ? String(localized: "Play \(targetCount) Albums")
            : String(localized: "Play")
    }

    var addToQueueLabel: String {
        targetCount > 1
            ? String(localized: "Add \(targetCount) Albums to Queue")
            : String(localized: "Add to Queue")
    }

    var addNextLabel: String {
        targetCount > 1
            ? String(localized: "Add \(targetCount) Albums Next")
            : String(localized: "Add Next")
    }

    var pinLabel: String {
        targetCount > 1
            ? String(localized: "Pin \(targetCount) Albums")
            : String(localized: "Pin")
    }
}
