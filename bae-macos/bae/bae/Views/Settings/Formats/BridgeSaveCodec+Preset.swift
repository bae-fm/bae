import BaeKit
import Foundation

extension BridgeSaveCodec {
    /// The format this codec saves as, as core reads it.
    var format: BridgeSaveFormat { bridgeSaveCodecFormat(codec: self) }

    /// Switch format, carrying the setting that still applies: bit depth
    /// across lossless formats, bitrate across lossy ones (clamped into the
    /// bitrates the target saves at). A switch between lossless and lossy
    /// takes the target's starting codec, all as core gives them.
    func switched(to format: BridgeSaveFormat) -> BridgeSaveCodec {
        let start = bridgeSaveFormatDefaultCodec(format: format)
        if let bitDepth, start.bitDepth != nil {
            return start.with(bitDepth: bitDepth)
        }
        if let bitrateKbps,
            let accepted = bridgeSaveFormatBitratesKbps(format: format)
        {
            return start.with(
                bitrateKbps: min(max(bitrateKbps, accepted.min), accepted.max)
            )
        }
        return start
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

    /// Whether this codec can write a release as one file with a CUE sheet,
    /// as core reads it.
    var supportsSingleFileCue: Bool {
        bridgeSaveCodecSupportsSingleFileCue(codec: self)
    }

    var label: String {
        switch self {
        case .flac, .wav, .aiff: format.label
        case .mp3(let bitrateKbps):
            String(localized: "MP3 \(Int(bitrateKbps)) kbps")
        case .aac(let bitrateKbps):
            String(localized: "AAC \(Int(bitrateKbps)) kbps")
        case .opusOgg(let bitrateKbps):
            String(localized: "Opus \(Int(bitrateKbps)) kbps")
        }
    }

    /// The extension of the files this codec writes, as core gives it.
    var fileExtension: String { bridgeSaveCodecExtension(codec: self) }
}
