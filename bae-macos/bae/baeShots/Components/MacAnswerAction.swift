import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAnswerAction`, each way to answer a folder that waits on the person.
    static let macAnswerActionScenes: [ShotScene] = [
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
}
