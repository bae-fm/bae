import BaeKit
import SwiftUI

extension EnvironmentValues {
    /// The network read an apply of a catalog release waits on, for a shot or
    /// a test with no bridge behind it: a line under it draws this step and
    /// subscribes to nothing. `nil` in the app.
    @Entry
    var heldCatalogApplyStep: BridgeCatalogApplyStep?
}

/// The network read an apply of a catalog release waits on, named over a bar
/// that counts nothing. The line reads the apply's row through the apply's
/// subscription, and takes no room while the apply waits on no read.
struct CatalogApplyStepLine: View {
    /// The apply whose step the line reads; `nil` where no bridge is behind
    /// it.
    let apply: BridgeCatalogApply?

    @Environment(\.heldCatalogApplyStep)
    private var held
    @Environment(UiStore.self)
    private var uiStore: UiStore?

    @State
    private var step: BridgeCatalogApplyStep?
    @State
    private var subscription: ScreenSubscription<CatalogApplyStepSubscription>?

    var body: some View {
        Group {
            if let shown = held ?? step {
                MacProgress(
                    title: shown.title,
                    bar: ProgressTrackBar(progress: shown.fraction)
                )
            }
        }
        .task(id: apply.map(ObjectIdentifier.init)) {
            step = nil
            subscription = nil
            guard held == nil, let apply else { return }
            subscription = ScreenSubscription(
                apply.subscribeStep(),
                onValue: { step = $0 },
                onError: { error in uiStore?.showError(error) }
            )
        }
    }
}
