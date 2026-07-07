import BaeKit
import Foundation

extension BridgeAudioFormat {
    /// One-line descriptor composed for the current locale, e.g.
    /// "FLAC · 44.1 kHz · 16-bit · stereo" (lossless) or
    /// "MP3 · 320 kbps · 44.1 kHz · stereo" (lossy). The codec is a proper noun;
    /// the channel word is localized; numbers use the locale's formatter. bae-core
    /// owns the parts and the lossy/lossless split (`bitsPerSample == nil`); this
    /// is the UI's locale rendering of them.
    var text: String {
        var parts = [codec]
        if bitsPerSample == nil, let kbps = bitrateKbps {
            parts.append("\(kbps.formatted()) kbps")
        }
        parts.append(sampleRateText)
        if let bits = bitsPerSample {
            parts.append("\(bits.formatted())-bit")
        }
        parts.append(channelsText)
        return parts.joined(separator: " · ")
    }

    private var sampleRateText: String {
        let khz = Double(sampleRateHz) / 1000.0
        let number = khz.formatted(.number.precision(.fractionLength(0...1)))
        return "\(number) kHz"
    }

    private var channelsText: String {
        if let key = bridgeAudioChannelsKey(channels: channels) {
            return NSLocalizedString(
                key,
                tableName: "Core",
                bundle: .main,
                comment: ""
            )
        }
        return "\(channels.formatted())ch"
    }
}
