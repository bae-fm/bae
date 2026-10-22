import BaeKit
import SwiftUI

/// A load-time failure as the shell and the reset screen show it: what
/// failed, the error with its details, and what to do next.
struct LoadErrorNotice: View {
    let loadError: LoadError

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            Text(verbatim: loadError.step.title)
                .themeText(.heading)
            ErrorDetailDisclosure(error: loadError.error)
            if let advice = loadError.step.advice {
                Text(verbatim: advice)
                    .themeText(.body)
                    .foregroundStyle(.secondary)
            }
        }
    }
}
