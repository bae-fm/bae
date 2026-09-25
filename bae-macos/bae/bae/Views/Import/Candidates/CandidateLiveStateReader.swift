import BaeKit
import SwiftUI

/// Draws `content` with what is running for one candidate and the commands its
/// row offers with it, kept current by the candidate's own subscription.
///
/// The list a row came from reads the tables alone, so a run moving from
/// queued to running or an import claiming the candidate reaches the row here
/// rather than through the list. `basis` is the row's own: a row the list
/// delivers again with a different one opens a new subscription for it.
/// `nil` until the subscription's first value, and wherever no importer is
/// mounted — a preview or a test with no bridge behind it.
struct CandidateLiveStateReader<Content: View>: View {
    let key: String
    let basis: BridgeCandidateActionBasis
    @ViewBuilder
    let content: (BridgeCandidateLiveState?) -> Content

    @Environment(Importer.self)
    private var importer: Importer?

    @State
    private var live: BridgeCandidateLiveState?

    private struct Subscription: Hashable {
        let key: String
        let basis: BridgeCandidateActionBasis
    }

    var body: some View {
        let subscription = Subscription(key: key, basis: basis)
        content(live)
            .task(id: subscription) {
                guard let importer else { return }
                for await value in importer.candidateLiveStates(
                    subscription.key,
                    basis: subscription.basis
                ) {
                    live = value
                }
            }
    }
}
