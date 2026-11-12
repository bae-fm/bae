import BaeKit
import SwiftUI

/// A device reset the opening screen follows: the counted step it runs and
/// where it stands.
struct MacWelcomeOpeningReset {
    let steps: LatestSource<BridgeBootstrapStep>
    let status: BridgeBootstrapStatus
}

/// What the main window shows while a call runs and no screen is ready: the
/// step it is on over a bar of how much of it is done, or `title` while no
/// step runs. `leading` is the call that came first (a close, a setup call),
/// then the library's open, then a device reset with where it stands.
struct MacWelcomeOpening<Leading: ProgressStep>: View {
    let title: LocalizedStringResource
    let leading: LatestSource<Leading>?
    let opening: LatestSource<BridgeOpenProgress>?
    let reset: MacWelcomeOpeningReset?

    var body: some View {
        MacProgressContainer(steps: leading) {
            MacProgressContainer(steps: opening) {
                if let reset {
                    MacProgressContainer(steps: reset.steps) {
                        BootstrapStatusView(status: reset.status)
                    }
                }
                else {
                    MacSpinner(Text(title))
                }
            }
        }
        .frame(maxWidth: StepProgressLayout.width)
    }
}
