import BaeKit

/// The codec families the Format picker offers.
enum SavePresetKind: CaseIterable {
    case flac
    case mp3
    case aac
    case opusOgg
    case wav
    case aiff

    /// Format names are proper nouns, shown as-is in every locale.
    var label: String {
        switch self {
        case .flac: "FLAC"
        case .mp3: "MP3"
        case .aac: "AAC"
        case .opusOgg: "Opus"
        case .wav: "WAV"
        case .aiff: "AIFF"
        }
    }

    var defaultCodec: BridgeSaveCodec {
        switch self {
        case .flac: .flac(bitDepth: .source)
        case .mp3: .mp3(bitrateKbps: 320)
        case .aac: .aac(bitrateKbps: 256)
        case .opusOgg: .opusOgg(bitrateKbps: 192)
        case .wav: .wav(bitDepth: .source)
        case .aiff: .aiff(bitDepth: .source)
        }
    }

    /// The bitrate range core's preset validation accepts for the lossy
    /// families; nil for lossless, which carry no bitrate.
    var bitrateRange: ClosedRange<UInt32>? {
        switch self {
        case .mp3: 32...320
        case .aac: 32...512
        case .opusOgg: 32...512
        case .flac, .wav, .aiff: nil
        }
    }
}
