import BaeKit
import Foundation

extension BridgeSaveCodec {
    var kind: SavePresetKind {
        switch self {
        case .flac: .flac
        case .mp3: .mp3
        case .aac: .aac
        case .opusOgg: .opusOgg
        case .wav: .wav
        case .aiff: .aiff
        }
    }

    /// Switch codec family, carrying the parameter that still applies: bit
    /// depth across lossless codecs, bitrate across lossy ones (clamped into
    /// the target family's supported range). A cross-family switch takes the
    /// family default.
    func switched(to kind: SavePresetKind) -> BridgeSaveCodec {
        switch kind {
        case .flac: .flac(bitDepth: bitDepth ?? .source)
        case .wav: .wav(bitDepth: bitDepth ?? .source)
        case .aiff: .aiff(bitDepth: bitDepth ?? .source)
        case .mp3: .mp3(bitrateKbps: min(max(bitrateKbps ?? 320, 32), 320))
        case .aac: .aac(bitrateKbps: min(max(bitrateKbps ?? 256, 32), 512))
        case .opusOgg: .opusOgg(bitrateKbps: bitrateKbps ?? 192)
        }
    }

    /// The lossless family's bit depth; nil for lossy codecs.
    var bitDepth: BridgeSaveBitDepth? {
        switch self {
        case .flac(let bitDepth), .wav(let bitDepth), .aiff(let bitDepth):
            bitDepth
        case .mp3, .aac, .opusOgg:
            nil
        }
    }

    /// The lossy family's bitrate; nil for lossless codecs.
    var bitrateKbps: UInt32? {
        switch self {
        case .mp3(let bitrateKbps), .aac(let bitrateKbps),
            .opusOgg(let bitrateKbps):
            bitrateKbps
        case .flac, .wav, .aiff:
            nil
        }
    }

    func with(bitDepth: BridgeSaveBitDepth) -> BridgeSaveCodec {
        switch self {
        case .flac: .flac(bitDepth: bitDepth)
        case .wav: .wav(bitDepth: bitDepth)
        case .aiff: .aiff(bitDepth: bitDepth)
        case .mp3, .aac, .opusOgg: self
        }
    }

    func with(bitrateKbps: UInt32) -> BridgeSaveCodec {
        switch self {
        case .mp3: .mp3(bitrateKbps: bitrateKbps)
        case .aac: .aac(bitrateKbps: bitrateKbps)
        case .opusOgg: .opusOgg(bitrateKbps: bitrateKbps)
        case .flac, .wav, .aiff: self
        }
    }

    var supportsSingleFileCue: Bool {
        switch self {
        case .aac, .opusOgg:
            false
        case .flac, .mp3, .wav, .aiff:
            true
        }
    }

    var label: String {
        switch self {
        case .flac: "FLAC"
        case .mp3(let bitrateKbps):
            String(localized: "MP3 \(Int(bitrateKbps)) kbps")
        case .aac(let bitrateKbps):
            String(localized: "AAC \(Int(bitrateKbps)) kbps")
        case .opusOgg(let bitrateKbps):
            String(localized: "Opus \(Int(bitrateKbps)) kbps")
        case .wav: "WAV"
        case .aiff: "AIFF"
        }
    }

    var fileExtension: String {
        switch self {
        case .flac: "flac"
        case .mp3: "mp3"
        case .aac: "m4a"
        case .opusOgg: "ogg"
        case .wav: "wav"
        case .aiff: "aiff"
        }
    }
}
