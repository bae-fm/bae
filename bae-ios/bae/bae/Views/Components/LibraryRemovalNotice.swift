import BaeKit
import SwiftUI

/// A library removal that failed, or that left this device's preferences for
/// a library it already left, with Retry for the latter. Nothing while every
/// removal finished.
struct LibraryRemovalNotice: View {
    @Environment(LibraryRemovals.self)
    private var removals

    var body: some View {
        if let error = removals.error {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text("Couldn't remove the library")
                    .themeText(.strong)
                ErrorDetailDisclosure(error: error, showIcon: false)
                if removals.unfinished != nil {
                    Button("Retry") { removals.retryUnfinished() }
                        .buttonStyle(.borderless)
                        .disabled(removals.isRetrying)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .notice(.danger)
        }
    }
}
