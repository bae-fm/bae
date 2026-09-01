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
        audioFactsText(parts)
    }

    fileprivate var parts: [String] {
        var parts = [codec]
        if bitsPerSample == nil, let kbps = bitrateKbps {
            parts.append(coreString("core.audio.bitrate_kbps", kbps))
        }
        parts.append(sampleRateText)
        if let bits = bitsPerSample {
            parts.append(coreString("core.audio.bit_depth", bits))
        }
        parts.append(channelsText)
        return parts
    }

    private var sampleRateText: String {
        let khz = Double(sampleRateHz) / 1000.0
        let number = khz.formatted(.number.precision(.fractionLength(0...1)))
        return coreString("core.audio.sample_rate_khz", number)
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
        return coreString("core.audio.channels.count", channels)
    }
}

extension BridgeSourceAudioDescriptor {
    var text: String {
        format.text
    }
}

extension BridgeSourceAudioSummary {
    var text: String {
        switch self {
        case .uniform(let descriptor):
            descriptor.text
        case .mixed:
            nonbreakingAudioFact(coreString("core.audio.mixed"))
        }
    }
}

private func audioFactsText(_ parts: [String]) -> String {
    parts.map(nonbreakingAudioFact)
        .joined(separator: coreString("core.audio.list_separator"))
}

private func nonbreakingAudioFact(_ text: String) -> String {
    text.replacingOccurrences(of: " ", with: "\u{00a0}")
        .replacingOccurrences(of: "-", with: "\u{2011}")
}
