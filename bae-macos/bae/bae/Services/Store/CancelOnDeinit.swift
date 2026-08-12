import BaeKit
import Foundation

/// Reference-type wrapper that cancels retained work when the wrapper is
/// dropped. Lets a value-type `Candidate` own tasks and live subscriptions:
/// removing or replacing the candidate drops the wrapper and cancels the work.
///
/// `Equatable` by identity so the synthesised `Candidate` equality still
/// works without comparing the retained work.
final class CancelOnDeinit: Equatable, Sendable {
    private let cancelWork: @Sendable () -> Void

    init(_ task: Task<Void, Never>) {
        cancelWork = { task.cancel() }
    }

    init(_ subscription: any LiveSubscriptionProtocol) {
        cancelWork = { subscription.cancel() }
    }

    deinit {
        cancelWork()
    }

    static func == (lhs: CancelOnDeinit, rhs: CancelOnDeinit) -> Bool {
        lhs === rhs
    }
}
