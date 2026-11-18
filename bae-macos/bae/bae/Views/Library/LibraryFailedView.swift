import BaeKit
import SwiftUI

/// A library view that failed whole, as the board draws it: the title says
/// what happened, with no glyph above it, then Details, which opens the
/// causes in an alert, and Retry when core says another try can succeed.
struct LibraryFailedView: View {
    let title: String
    let error: DisplayError
    let retry: () -> Void

    @State
    private var openedDetails: ErrorDetails?

    var body: some View {
        ContentUnavailableView {
            Text(verbatim: title)
        } actions: {
            if let details = ErrorDetails(
                title: title,
                advice: nil,
                error: error
            ) {
                Button("Details") { openedDetails = details }
            }
            if error.offersRetry {
                Button("Retry", action: retry)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .errorDetailsAlert($openedDetails)
    }
}
