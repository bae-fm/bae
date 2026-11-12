import BaeKit
import SwiftUI

/// A restore running: the step it is on over a bar of how much of it is
/// done, with Cancel. It takes the whole window, as a library opening does.
struct CodeRunningView: View {
    let openSteps: LatestSource<BridgeOpenProgress>?
    let steps: LatestSource<BridgeBootstrapStep>?
    let status: BridgeBootstrapStatus?
    let onCancel: () -> Void

    var body: some View {
        VStack(spacing: ThemeSpace.section) {
            if let status {
                MacProgressContainer(steps: openSteps, look: .stage) {
                    MacProgressContainer(steps: steps, look: .stage) {
                        BootstrapStatusView(status: status)
                    }
                }
                .frame(maxWidth: StepProgressLayout.width)
            }
            Button("Cancel", action: onCancel)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
