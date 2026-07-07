import BaeKit
import Foundation

// MARK: - FileInfo

struct FileInfo: Equatable {
    /// Relative path within the candidate root, e.g. "Disc 1/track.flac".
    let name: String
    /// File size in bytes. bae-core emits the raw count; the UI formats it.
    let size: UInt64
    /// Directory portion of `name` with trailing slash. `nil` when the file
    /// sits at the candidate-folder root.
    let dirPrefix: String?
    /// File name without directory.
    let fileName: String
    /// Absolute filesystem path of the file on disk.
    let localPath: String

    /// File size formatted for the current locale, e.g. "35 MB".
    var sizeText: String {
        Int64(size).formatted(.byteCount(style: .file))
    }

    init(bridge: BridgeFileInfo) {
        name = bridge.name
        size = bridge.size
        dirPrefix = bridge.dirPrefix
        fileName = bridge.fileName
        localPath = bridge.localPath
    }
}

// MARK: - ArtworkFile

struct ArtworkFile: Equatable {
    let file: FileInfo
    let coverChoice: BridgeCoverChoice

    var name: String { file.name }
    var localPath: String { file.localPath }

    init(bridge: BridgeArtworkFile) {
        file = FileInfo(bridge: bridge.file)
        coverChoice = bridge.coverChoice
    }
}

// MARK: - CueFlacPair

struct CueFlacPair: Equatable {
    let cueName: String
    /// CUE file size in bytes. The UI formats it.
    let cueSize: UInt64
    /// Absolute filesystem path of the CUE file on disk.
    let cueLocalPath: String
    let flacName: String
    /// Absolute filesystem path of the audio file on disk.
    let flacLocalPath: String
    /// Combined CUE + audio size in bytes. The UI formats it.
    let totalSize: UInt64
    let trackCount: UInt32

    /// CUE file size formatted for the current locale, e.g. "1 KB".
    var cueSizeText: String {
        Int64(cueSize).formatted(.byteCount(style: .file))
    }

    /// Combined size formatted for the current locale, e.g. "340 MB".
    var totalSizeText: String {
        Int64(totalSize).formatted(.byteCount(style: .file))
    }

    init(bridge: BridgeCueFlacPair) {
        cueName = bridge.cueName
        cueSize = bridge.cueSize
        cueLocalPath = bridge.cueLocalPath
        flacName = bridge.flacName
        flacLocalPath = bridge.flacLocalPath
        totalSize = bridge.totalSize
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
    var artwork: [ArtworkFile]
    var documents: [FileInfo]

    init(bridge: BridgeCandidateFiles) {
        audio = AudioContent(bridge: bridge.audio)
        artwork = bridge.artwork.map(ArtworkFile.init(bridge:))
        documents = bridge.documents.map(FileInfo.init(bridge:))
    }

    init(audio: AudioContent, artwork: [ArtworkFile], documents: [FileInfo]) {
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
