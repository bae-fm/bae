import BaeKit
import SwiftUI

@testable import bae

// The import's answer buttons, its search source lines and the modal header.

extension ShotScene {
    static let importAnswerScenes: [ShotScene] =
        answerActionScenes + searchSourceLineScenes + modalHeaderScenes

    private static func answerAction(
        _ id: String,
        title: String,
        subtitle: String,
        off: Bool = false
    ) -> ShotScene {
        component(id, height: 70) {
            MacAnswerAction(
                title: title,
                subtitle: subtitle,
                off: off,
                action: {}
            )
        }
    }

    /// `MacAnswerAction`, each way to answer a folder that waits on the person.
    private static let answerActionScenes: [ShotScene] = [
        answerAction(
            "mac.component.answer-action.none-of-these",
            title: "None of these",
            subtitle: "Keep my info; it is not in the catalogs"
        ),
        answerAction(
            "mac.component.answer-action.keep-my-info",
            title: "Keep my info",
            subtitle: "Keep my info; it is not in the catalogs"
        ),
        answerAction(
            "mac.component.answer-action.not-sure",
            title: "Not sure",
            subtitle: "Use what these releases share; leave the rest as is"
        ),
        answerAction(
            "mac.component.answer-action.while-importing",
            title: "None of these",
            subtitle: "Keep my info; it is not in the catalogs",
            off: true
        ),
    ]

    /// `MacSearchSourceLine`, a catalog a typed search still waits on.
    private static let searchSourceLineScenes: [ShotScene] = [
        component("mac.component.search-source-line.searching", height: 40) {
            MacSearchSourceLine(
                catalog: "Discogs",
                status: .searching,
                onRetry: {}
            )
        },
        component("mac.component.search-source-line.failed", height: 40) {
            MacSearchSourceLine(
                catalog: "Discogs",
                status: .failed(reason: "Discogs did not answer in time."),
                onRetry: {}
            )
        },
        component(
            "mac.component.search-source-line.another-catalog",
            height: 40
        ) {
            MacSearchSourceLine(
                catalog: "MusicBrainz",
                status: .searching,
                onRetry: {}
            )
        },
    ]

    /// `MacModalHeader`, a sheet's title with its second line and button.
    private static let modalHeaderScenes: [ShotScene] = [
        component("mac.component.modal-header.default", width: 544, height: 80)
        {
            MacModalHeader(
                title: "Edit release",
                subtitle: "Low Tide Hours · CD"
            ) {
                Button("Close") {}
            }
        }
    ]
}
