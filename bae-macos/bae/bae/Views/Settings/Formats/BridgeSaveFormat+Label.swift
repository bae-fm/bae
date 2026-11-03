import BaeKit

extension BridgeSaveFormat {
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
}
