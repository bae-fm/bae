import BaeKit

/// Which problems the Activity panel lists and which of them the person has
/// dismissed: the app's own, then a failure to read sync's state, then sync's
/// failure. A dismissed problem stays out of the panel and the button's
/// warning while it stays listed.
struct ActivityProblems: Equatable {
    /// Every problem listed, in the panel's order.
    let listed: [ActivityProblemID]
    /// The listed problems the person hasn't dismissed.
    let shown: [ActivityProblemID]
    /// Pending records are listed. They have no ×, so they are never
    /// dismissed.
    let recordsListed: Bool

    init(
        app: [AppProblem],
        activity: BridgeSyncActivity?,
        error: DisplayError?,
        dismissed: Set<ActivityProblemID>
    ) {
        var listed = app.map(\.id)
        if error != nil {
            listed.append(.syncStatusUnread)
        }
        if activity?.failure.flatMap({ DisplayError($0) }) != nil {
            listed.append(.syncFailure)
        }
        self.listed = listed
        shown = listed.filter { !dismissed.contains($0) }
        recordsListed = activity?.attention.isEmpty == false
    }

    var needsAttention: Bool {
        !shown.isEmpty || recordsListed
    }
}
