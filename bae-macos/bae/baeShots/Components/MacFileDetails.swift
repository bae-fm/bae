import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacFileDetails`, an audio file's with its folder and format and a
    /// log's with neither. The audio file's kind reads "FLAC audio": the
    /// component says "Audio".
    static let macFileDetailsScenes: [ShotScene] = [
        component("mac.component.file-details.audio-file", height: 200) {
            MacFileDetails(
                file: BridgeFile(
                    id: "file-flac",
                    originalFilename: "01 Harbor Lights.flac",
                    name: "01 Harbor Lights.flac",
                    folder: "Mara Quill/Low Tide Hours",
                    kind: .audio(format: "FLAC"),
                    fileSize: 34_100_000,
                    contentType: "audio/flac",
                    isImage: false,
                    audioFormat: BridgeAudioFormat(
                        codec: "FLAC",
                        sampleRateHz: 44_100,
                        bitsPerSample: 16,
                        bitrateKbps: nil,
                        channels: 2
                    )
                ),
                onClose: {}
            )
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.file-details.other-file", height: 160) {
            MacFileDetails(
                file: BridgeFile(
                    id: "file-log",
                    originalFilename: "Field Recordings.log",
                    name: "Field Recordings.log",
                    folder: nil,
                    kind: .ripLog,
                    fileSize: 6_000,
                    contentType: "text/plain",
                    isImage: false,
                    audioFormat: nil
                ),
                onClose: {}
            )
            .frame(width: 380, alignment: .leading)
        },
    ]
}
