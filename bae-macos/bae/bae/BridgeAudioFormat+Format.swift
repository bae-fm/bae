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
        parts.append(sampleRateText(hz: Double(sampleRateHz)))
        if let bits = bitsPerSample {
            parts.append(coreString("core.audio.bit_depth", bits))
        }
        parts.append(channelsText(channels))
        return parts
    }
}

extension BridgeSourceAudioDescriptor {
    var text: String {
        format.text
    }
}

extension BridgeSourceAudioSummary {
    /// One format's facts, or the facts the files disagree on, each fact's
    /// values joined by the locale's list formatter ("FLAC, MP3").
    var text: String {
        switch self {
        case .uniform(let descriptor):
            descriptor.text
        case .mixed(let differences):
            differences.map(\.text)
                .joined(separator: coreString("core.audio.list_separator"))
        }
    }
}

extension BridgeSourceAudioDifference {
    var text: String {
        ListFormatter.localizedString(
            byJoining: values.map(nonbreakingAudioFact)
        )
    }

    private var values: [String] {
        switch self {
        case .layout(let layouts):
            layouts.map {
                switch $0 {
                case .cue: coreString("core.audio.layout.cue")
                case .file: coreString("core.audio.layout.file")
                }
            }
        case .codec(let codecs):
            codecs
        case .sampleRate(let sampleRatesHz):
            sampleRatesHz.map { sampleRateText(hz: Double($0)) }
        case .bitDepth(let bitsPerSample):
            bitsPerSample.map { coreString("core.audio.bit_depth", $0) }
        case .channels(let channels):
            channels.map(channelsText)
        }
    }
}

/// A channel count's word ("stereo"), or its count where it has none.
private func channelsText(_ channels: Int64) -> String {
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

/// A sample rate in kilohertz for the current locale, e.g. "44.1 kHz".
func sampleRateText(hz: Double) -> String {
    let number = (hz / 1000.0)
        .formatted(.number.precision(.fractionLength(0...1)))
    return coreString("core.audio.sample_rate_khz", number)
}

private func audioFactsText(_ parts: [String]) -> String {
    parts.map(nonbreakingAudioFact)
        .joined(separator: coreString("core.audio.list_separator"))
}

private func nonbreakingAudioFact(_ text: String) -> String {
    text.replacingOccurrences(of: " ", with: "\u{00a0}")
        .replacingOccurrences(of: "-", with: "\u{2011}")
}
