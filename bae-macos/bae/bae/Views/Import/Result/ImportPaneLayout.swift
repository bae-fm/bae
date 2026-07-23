import CoreGraphics

/// Sizing for the docked confirm pane, shared between the pane (drag clamp) and
/// its container (frame height) so they never disagree. Non-generic so the
/// container can call `clamp` without pinning the pane's `Content` type.
enum ImportPaneLayout {
    static let minHeight: CGFloat = 220
    static let maxHeight: CGFloat = 500
    /// Keep at least this much of the results list visible above the pane.
    private static let resultsFloor: CGFloat = 140

    /// Clamp a requested height to [min, capped-max], where the cap also
    /// reserves the results floor.
    static func clamp(_ requested: CGFloat, available: CGFloat) -> CGFloat {
        let cap = max(minHeight, min(maxHeight, available - resultsFloor))
        return max(minHeight, min(cap, requested))
    }
}
