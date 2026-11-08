import BaeKit
import SwiftUI

/// A load-time failure as the shell and the reset screen show it: what
/// failed, the error with its details, and what to do next.
struct LoadErrorNotice: View {
    let loadError: LoadError

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            StatusGlyph(.danger)
                .themeIcon(.medium)
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(verbatim: loadError.step.title)
                    .themeText(.heading)
                ErrorDetailDisclosure(error: loadError.error, bare: true)
                if let advice = loadError.step.advice {
                    Text(verbatim: advice)
                        .themeText(.body)
                        .foregroundStyle(NoticeText.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .notice(.danger)
    }
}
