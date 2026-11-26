import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macFindOnlineScenes: [ShotScene] = [
        component(
            "mac.component.find-online.default",
            width: 842,
            height: 340,
            images: PreviewData.artImageStore()
        ) {
            findOnline(height: 340)
        },
        component(
            "mac.component.find-online.no-discogs-bar",
            width: 842,
            height: 240,
            images: PreviewData.artImageStore()
        ) {
            findOnline(height: 240, showsDiscogsBar: false)
        },
        component(
            "mac.component.find-online.not-started",
            width: 842,
            height: 280,
            images: PreviewData.artImageStore()
        ) {
            findOnline(
                height: 280,
                state: PreviewData.searchStateIdle,
                atContentHeight: true
            )
        },
        component(
            "mac.component.find-online.working",
            width: 842,
            height: 256,
            images: PreviewData.artImageStore()
        ) {
            findOnline(
                height: 256,
                state: PreviewData.searchState(
                    identifyState: .triangulating(
                        run: PreviewData.identifyRunNothingFound,
                        groups: [],
                        agreements: [:],
                        narrowedOutCount: 0
                    ),
                    signals: PreviewData.settledSignals
                ),
                atContentHeight: true
            )
        },
        component(
            "mac.component.find-online.failed",
            width: 842,
            height: 256,
            images: PreviewData.artImageStore()
        ) {
            findOnline(
                height: 256,
                state: PreviewData.searchState(
                    identifyState: .failed(
                        run: PreviewData.identifyRunNothingFound,
                        failureLines: nil,
                        groups: [],
                        agreements: [:],
                        narrowedOutCount: 0,
                        offersSharedAlbum: false
                    ),
                    signals: PreviewData.settledSignals
                ),
                atContentHeight: true
            )
        },
        component(
            "mac.component.find-online.sentence",
            width: 842,
            height: 381,
            images: PreviewData.artImageStore()
        ) {
            findOnline(
                height: 381,
                state: PreviewData.searchStateNotFoundNeedsYou,
                keepsOwnDraft: false
            )
        },
        component(
            "mac.component.find-online.nothing-found-an-answer",
            width: 842,
            height: 431,
            images: PreviewData.artImageStore()
        ) {
            findOnline(
                height: 431,
                state: PreviewData.searchStateNotFoundNeedsYou
            )
        },
    ]

    /// The pane at the height its board draws, which is the board's content.
    private static func findOnline(
        height: CGFloat,
        state: ImportSearchState = PreviewData.searchStateNotFound,
        showsDiscogsBar: Bool = true,
        keepsOwnDraft: Bool = true,
        atContentHeight: Bool = false
    ) -> some View {
        importStores(
            MacFindOnline.preview(
                state: state,
                showsDiscogsBar: showsDiscogsBar,
                keepsOwnDraft: keepsOwnDraft
            )
            .frame(width: 842)
            // The app's window never makes the pane as short as its content,
            // and a pane that short cuts the Discogs bar's words.
            .fixedSize(horizontal: false, vertical: atContentHeight)
            .frame(height: height, alignment: .top)
            // The import pane's surface, which the board draws under it.
            .background(Theme.surface)
        )
    }
}
