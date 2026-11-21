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
        }
    ]

    private static func findOnline() -> some View {
        importStores(
            MacFindOnline.preview(
                state: PreviewData.searchStateNotFound,
                showsDiscogsBar: true
            )
            .frame(width: 580, height: 330, alignment: .top)
        )
    }
}
