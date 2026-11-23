import BaeKit
import SwiftUI

/// What belongs to the row above it in a settings box: no line between them
/// and none of the space a row of its own has above it, so the two read as one
/// setting with a second line.
struct MacNested<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        content.containerValue(\.joinsRowAbove, true)
    }
}
