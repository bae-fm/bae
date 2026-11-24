import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macFindOnlineScenes: [ShotScene] = [
        component(
            "mac.component.find-online.default",
            width: 842,
            height: 450,
            images: PreviewData.artImageStore()
        ) {
            findOnline()
        },
        component(
            "mac.component.find-online.no-discogs-bar",
            width: 842,
            height: 450,
            images: PreviewData.artImageStore()
        ) {
            findOnline(showsDiscogsBar: false)
        },
        component(
            "mac.component.find-online.not-started",
            page: true,
            width: 842,
            height: 450,
            images: PreviewData.artImageStore()
        ) {
            findOnline(state: PreviewData.searchStateIdle)
        },
        component(
            "mac.component.find-online.sentence",
            width: 842,
            height: 450,
            images: PreviewData.artImageStore()
        ) {
            findOnline(
                state: PreviewData.searchStateNotFoundNeedsYou,
                keepsOwnDraft: false
            )
        },
        component(
            "mac.component.find-online.nothing-found-an-answer",
            width: 842,
            height: 450,
            images: PreviewData.artImageStore()
        ) {
            findOnline(state: PreviewData.searchStateNotFoundNeedsYou)
        },
    ]

    private static func findOnline(
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
            .frame(width: 842, height: 420, alignment: .top)
        )
    }
}
