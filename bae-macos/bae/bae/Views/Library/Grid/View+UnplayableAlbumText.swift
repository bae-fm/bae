import BaeKit
import SwiftUI

extension View {
    /// Dims the words of an album card whose release this Mac can't play;
    /// `names` also tells VoiceOver so, on the card's title.
    func unplayableAlbumText(
        _ isUnplayable: Bool,
        names: Bool = false
    ) -> some View {
        opacity(isUnplayable ? ThemeOpacity.dimmed : 1)
            .accessibilityHint(
                "Not on this device",
                isEnabled: isUnplayable && names
            )
    }
}
