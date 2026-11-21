import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macFindOnlineScenes: [ShotScene] = [
        component(
            "mac.component.find-online.default",
            page: true,
            width: 580,
            height: 360,
            images: PreviewData.artImageStore()
        ) {
            findOnline()
        },
        component(
            "mac.component.find-online.no-discogs-bar",
            page: true,
            width: 580,
            height: 360,
            images: PreviewData.artImageStore()
        ) {
            findOnline(showsDiscogsBar: false)
        },
    ]

    private static func findOnline(showsDiscogsBar: Bool = true) -> some View {
        importStores(
            MacFindOnline.preview(
                state: PreviewData.searchStateNotFound,
                showsDiscogsBar: showsDiscogsBar
            )
            .frame(width: 580, height: 330, alignment: .top)
        )
    }
}
