#if DEBUG
    import BaeKit

    // The save-format presets and filename patterns a preview's
    // `ConfigStore` starts with.
    extension PreviewData {
        // MARK: - Formats

        /// The file name pattern a new library starts with, for seeding
        /// shot configs (mirrors bae-preferences' default).
        static let exportFilenamePieces: [BridgeSaveFilenamePiece] = [
            .value(value: .trackNumber), .text(text: " "),
            .value(value: .title),
        ]
        /// The folder name pattern a new library starts with.
        static let exportFolderPieces: [BridgeSaveFolderPiece] = [
            .value(value: .artist), .text(text: " - "), .value(value: .album),
        ]
        /// The cover and loudness settings a new library starts with: all on.
        static let saveExtras = BridgeSaveExtras(
            embedCover: true,
            coverFile: true,
            loudnessTags: true
        )
        static let savePresets: [BridgeSavePreset] = [
            BridgeSavePreset(
                id: "flac",
                name: "FLAC",
                codec: .flac(bitDepth: .sameAsSource)
            ),
            BridgeSavePreset(
                id: "mp3",
                name: "MP3",
                codec: .mp3(bitrateKbps: 320)
            ),
        ]

    }
#endif
