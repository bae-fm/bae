import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macReleaseFileRowScenes: [ShotScene] = [
        releaseFileRow(
            "mac.component.release-file-row.opens",
            name: "rip.log",
            size: "4 KB",
            opens: true
        ),
        releaseFileRow(
            "mac.component.release-file-row.other",
            name: ".DS_Store",
            size: "6 KB"
        ),
        releaseFileRow(
            "mac.component.release-file-row.damaged",
            name: "02 Margins.flac",
            size: "31.8 MB",
            damage: "Cannot decode"
        ),
        releaseFileRow(
            "mac.component.release-file-row.audio",
            name: "01 Northbound.flac",
            size: "33 MB",
            plays: true
        ),
        releaseFileRow(
            "mac.component.release-file-row.long-name",
            name:
                "Mara Quill - Low Tide Hours - 1992 - EAC rip log with the drive offset and every read.log",
            size: "4 KB",
            opens: true
        ),
    ]

    /// `MacReleaseFileRow`, a file of a release at the width the preview
    /// gives it: a document that opens, a file that opens nothing, a damaged
    /// one, an audio file with its Play button, and a long name.
    private static func releaseFileRow(
        _ id: String,
        name: String,
        size: String,
        damage: String? = nil,
        opens: Bool = false,
        plays: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 480,
            height: 40
        ) {
            MacReleaseFileRow(
                name: name,
                size: size,
                damage: damage,
                onOpen: opens ? {} : nil,
                onPlay: plays ? {} : nil
            )
            .frame(width: 480, alignment: .leading)
        }
    }
}
