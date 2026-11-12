import SwiftUI

/// A disclosure chevron that points toward the reading direction when closed
/// and turns a quarter to point down when open, in either layout direction.
struct DisclosureChevron: View {
    let isExpanded: Bool

    @Environment(\.layoutDirection)
    private var layoutDirection

    var body: some View {
        Image(systemName: "chevron.forward")
            .rotationEffect(.degrees(angle))
    }

    /// `chevron.forward` points left in a right-to-left layout, so its
    /// quarter turn to the bottom runs the other way.
    private var angle: Double {
        guard isExpanded else { return 0 }
        return layoutDirection == .rightToLeft ? -90 : 90
    }
}
