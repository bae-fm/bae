import BaeKit
import SwiftUI

/// Shown when the import list could not be read, instead of an empty state
/// that would wrongly say no folder is watched. Details opens the causes;
/// Retry reads the list again, and is offered unless core says making the
/// same call again cannot get past the failure.
struct ImportListFailedView: View {
    let failure: DisplayError
    let onRetry: () -> Void

    var body: some View {
        Notice(
            title: String(localized: "The import list could not be read"),
            advice: nil,
            error: failure,
            look: .centered,
            action: failure.retryAction(perform: onRetry)
        )
    }
}
