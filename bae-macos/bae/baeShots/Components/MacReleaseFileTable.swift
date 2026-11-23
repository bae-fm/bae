import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacReleaseFileTable`, a release's files as a table of name, audio
    /// format, size and kind, sorted by name and by size, largest first. The
    /// Storage sheet draws the Files heading over the table.
    static let macReleaseFileTableScenes: [ShotScene] = [
        releaseFileTable(
            "mac.component.release-file-table.files",
            [harborLightsFile, longWayHomeFile, coverFile, cueFile, logFile],
            height: 190
        ),
        releaseFileTable(
            "mac.component.release-file-table.sorted-by-size",
            [longWayHomeFile, harborLightsFile, coverFile, logFile, cueFile],
            order: BridgeReleaseFileOrder(column: .size, descending: true),
            height: 190
        ),
        releaseFileTable(
            "mac.component.release-file-table.long-name",
            [
                releaseFile(
                    "01 Harbor Lights (Recorded Live at the Harbor Hall, Second Night).flac",
                    audio: BridgeAudioFormat(
                        codec: "FLAC",
                        sampleRateHz: 96_000,
                        bitsPerSample: 24,
                        bitrateKbps: nil,
                        channels: 2
                    ),
                    size: 98_400_000,
                    contentType: "audio/flac"
                )
            ],
            height: 90
        ),
    ]

    private static let byName = BridgeReleaseFileOrder(
        column: .name,
        descending: false
    )

    /// `files` in the order core returns them for `order`.
    private static func releaseFileTable(
        _ id: String,
        _ files: [BridgeFile],
        order: BridgeReleaseFileOrder = byName,
        height: CGFloat
    ) -> ShotScene {
        component(
            id,
            width: 544,
            height: height
        ) {
            MacReleaseFileTable(
                files: files,
                order: .constant(order),
                presentationError: .constant(nil)
            )
            .frame(width: 544, alignment: .leading)
        }
    }

    private static let flac1644 = BridgeAudioFormat(
        codec: "FLAC",
        sampleRateHz: 44_100,
        bitsPerSample: 16,
        bitrateKbps: nil,
        channels: 2
    )

    private static let harborLightsFile = releaseFile(
        "01 Harbor Lights.flac",
        audio: flac1644,
        size: 34_100_000,
        contentType: "audio/flac"
    )

    private static let longWayHomeFile = releaseFile(
        "02 The Long Way Home.flac",
        audio: flac1644,
        size: 38_900_000,
        contentType: "audio/flac"
    )

    private static let coverFile = releaseFile(
        "cover.jpg",
        size: 1_200_000,
        contentType: "image/jpeg"
    )

    private static let cueFile = releaseFile(
        "Low Tide Hours.cue",
        size: 2_000,
        contentType: "application/x-cue"
    )

    private static let logFile = releaseFile(
        "Low Tide Hours.log",
        size: 6_000,
        contentType: "text/plain"
    )
}
