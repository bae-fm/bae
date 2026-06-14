import Foundation

// MARK: - FileInfo

struct FileInfo: Equatable {
    /// Relative path within the candidate root, e.g. "Disc 1/track.flac".
    let name: String
    let sizeLabel: String
    /// Directory portion of `name` with trailing slash. `nil` when the file
    /// sits at the candidate-folder root.
    let dirPrefix: String?
    /// File name without directory.
    let fileName: String
    /// Absolute filesystem path of the file on disk.
    let localPath: String

    init(bridge: BridgeFileInfo) {
        name = bridge.name
        sizeLabel = bridge.sizeLabel
        dirPrefix = bridge.dirPrefix
        fileName = bridge.fileName
        localPath = bridge.localPath
    }
}

// MARK: - CueFlacPair

struct CueFlacPair: Equatable {
    let cueName: String
    let cueSizeLabel: String
    /// Absolute filesystem path of the CUE file on disk.
    let cueLocalPath: String
    let flacName: String
    /// Absolute filesystem path of the audio file on disk.
    let flacLocalPath: String
    let totalSizeLabel: String
    /// `nil` when the CUE hasn't been parsed yet.
    let trackCount: UInt32?

    init(bridge: BridgeCueFlacPair) {
        cueName = bridge.cueName
        cueSizeLabel = bridge.cueSizeLabel
        cueLocalPath = bridge.cueLocalPath
        flacName = bridge.flacName
        flacLocalPath = bridge.flacLocalPath
        totalSizeLabel = bridge.totalSizeLabel
        trackCount = bridge.trackCount
    }
}

// MARK: - AudioContent

enum AudioContent: Equatable {
    case cueFlacPairs(pairs: [CueFlacPair])
    case trackFiles(files: [FileInfo])

    init(bridge: BridgeAudioContent) {
        switch bridge {
        case .cueFlacPairs(let pairs):
            self = .cueFlacPairs(pairs: pairs.map(CueFlacPair.init(bridge:)))
        case .trackFiles(let files):
            self = .trackFiles(files: files.map(FileInfo.init(bridge:)))
        }
    }
}

// MARK: - CandidateFiles

struct CandidateFiles: Equatable {
    var audio: AudioContent
    var artwork: [FileInfo]
    var documents: [FileInfo]

    init(bridge: BridgeCandidateFiles) {
        audio = AudioContent(bridge: bridge.audio)
        artwork = bridge.artwork.map(FileInfo.init(bridge:))
        documents = bridge.documents.map(FileInfo.init(bridge:))
    }

    init(audio: AudioContent, artwork: [FileInfo], documents: [FileInfo]) {
        self.audio = audio
        self.artwork = artwork
        self.documents = documents
    }

    /// Empty file set — for candidates not delivered through the scanner's
    /// scan-event channel (re-identify reads its files from the DB).
    static let empty = CandidateFiles(
        audio: .trackFiles(files: []),
        artwork: [],
        documents: []
    )

}
