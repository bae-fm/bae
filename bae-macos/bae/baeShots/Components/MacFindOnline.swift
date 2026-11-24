import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macFindOnlineScenes: [ShotScene] = [
        component(
            "mac.component.find-online.default",
            width: 842,
            height: 373,
            images: PreviewData.artImageStore()
        ) {
            findOnline(height: 373)
        },
        component(
            "mac.component.find-online.no-discogs-bar",
            width: 842,
            height: 273,
            images: PreviewData.artImageStore()
        ) {
            findOnline(height: 273, showsDiscogsBar: false)
        },
        component(
            "mac.component.find-online.not-started",
            page: true,
            width: 842,
            height: 264,
            images: PreviewData.artImageStore()
        ) {
            findOnline(height: 264, state: PreviewData.searchStateIdle)
        },
        component(
            "mac.component.find-online.sentence",
            width: 842,
            height: 414,
            images: PreviewData.artImageStore()
        ) {
            findOnline(
                height: 414,
                state: PreviewData.searchStateNotFoundNeedsYou,
                keepsOwnDraft: false
            )
        },
        component(
            "mac.component.find-online.nothing-found-an-answer",
            width: 842,
            height: 464,
            images: PreviewData.artImageStore()
        ) {
            findOnline(
                height: 464,
                state: PreviewData.searchStateNotFoundNeedsYou
            )
        },
    ]

    /// The pane at the height its board draws, which is the board's content.
    private static func findOnline(
        height: CGFloat,
        state: ImportSearchState = PreviewData.searchStateNotFound,
        showsDiscogsBar: Bool = true,
        keepsOwnDraft: Bool = true
    ) -> some View {
        importStores(
            MacFindOnline.preview(
                state: state,
                showsDiscogsBar: showsDiscogsBar,
                keepsOwnDraft: keepsOwnDraft
            )
            .frame(width: 842, height: height, alignment: .top)
        )
    }
}
