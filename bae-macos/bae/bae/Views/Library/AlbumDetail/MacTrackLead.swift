import AppKit
import BaeKit
import Combine
import SwiftUI

/// A track's position as the track list draws it, on one line: the rows show
/// it, and the list measures every one to size the rows' number column.
struct MacTrackLead: View {
    let text: String

    var body: some View {
        Text(text)
            .themeText(.detail)
            .monospacedDigit()
            .lineLimit(1)
            .fixedSize()
    }
}
