import BaeKit
import SwiftUI

@testable import bae

extension BridgeCoverGallery {
    /// The gallery as core focuses it for a request that names `key`: on
    /// that cover while the gallery shows it, else on its first cover, with
    /// the covers before and after it wrapping around the ends.
    func focused(on key: BridgeCoverKey?) -> BridgeCoverGallery {
        let covers = [current].compactMap { $0 } + sections.flatMap(\.items)
        var gallery = self
        let place = covers.firstIndex { $0.key == key } ?? covers.indices.first
        gallery.focus = place.map { place in
            let lone = covers.count == 1
            return BridgeGalleryFocus(
                cover: covers[place],
                position: UInt32(place),
                previous: lone
                    ? nil
                    : covers[(place + covers.count - 1) % covers.count].key,
                next: lone ? nil : covers[(place + 1) % covers.count].key
            )
        }
        return gallery
    }
}

/// A gallery view over `gallery` that moves its focus as core would: each
/// cover the view focuses is a new request, which this view answers with
/// `gallery` focused on it.
struct FocusingCoverGallery: View {
    let gallery: BridgeCoverGallery
    let view:
        (CoverGalleryContent, Binding<BridgeCoverKey?>) -> CoverGalleryView

    @State
    private var focusedCover: BridgeCoverKey?

    var body: some View {
        view(
            CoverGalleryContent(gallery.focused(on: focusedCover)),
            $focusedCover
        )
    }
}
