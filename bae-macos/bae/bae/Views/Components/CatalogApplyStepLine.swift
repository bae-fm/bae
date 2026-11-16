import BaeKit
import SwiftUI

/// The network read an apply of a catalog release waits on: the step's title
/// over a progress bar that shows no fraction. The line reads the step from
/// `apply`. While the apply waits on no read, the line stays in the layout
/// with no height and draws nothing.
struct CatalogApplyStepLine: View {
    /// Where the line reads the apply's step; `nil` while no apply runs.
    let apply: (any CatalogApplySource)?

    var body: some View {
        SubscriptionReader(
            id: apply.map(ObjectIdentifier.init),
            subscribe: { apply?.stepValues() },
            content: Self.line
        )
    }

    /// The line for `step`: its title over the bar, or nothing, with no
    /// height, while there is no step.
    private static func line(_ step: BridgeCatalogApplyStep?) -> some View {
        MacProgress(
            title: step?.title ?? "",
            bar: ProgressTrackBar(progress: step?.fraction)
        )
        .frame(height: step == nil ? 0 : nil, alignment: .top)
        .clipped()
        .opacity(step == nil ? 0 : 1)
        .accessibilityHidden(step == nil)
    }
}
