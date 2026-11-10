import BaeKit
import Foundation

extension BridgeSaveCodec {
    /// The format this codec saves as, as core reads it.
    var format: BridgeSaveFormat { bridgeSaveCodecFormat(codec: self) }

    /// This codec under another format, as core carries its setting over.
    func switched(to format: BridgeSaveFormat) -> BridgeSaveCodec {
        bridgeSaveCodecSwitchedTo(codec: self, format: format)
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

    /// The extension of the files this codec writes, as core gives it.
    var fileExtension: String { bridgeSaveCodecExtension(codec: self) }
}
