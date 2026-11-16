import BaeKit
import SwiftUI

/// Draws `content` with the latest value of the subscription `subscribe`
/// makes, and `nil` before its first value. Each time `id` changes, the
/// reader ends the subscription it holds and makes a new one; when
/// `subscribe` makes none, the reader draws `nil`. A failed pull shows on
/// the window's error line.
struct SubscriptionReader<
    ID: Hashable,
    Source: PulledSubscription,
    Content: View
>: View {
    let id: ID
    let subscribe: () -> Source?
    @ViewBuilder
    let content: (Source.Value?) -> Content

    @Environment(UiStore.self)
    private var uiStore: UiStore?

    @State
    private var latest: Source.Value?
    @State
    private var subscription: ScreenSubscription<Source>?

    var body: some View {
        content(latest)
            .task(id: id) {
                latest = nil
                subscription = nil
                guard let source = subscribe() else { return }
                subscription = ScreenSubscription(
                    source,
                    onValue: { latest = $0 },
                    onError: { error in uiStore?.showError(error) }
                )
            }
    }
}
