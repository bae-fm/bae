import BaeKit
import Testing

@testable import bae

/// Switching a preset's format keeps the setting that still applies, brought
/// into core's range for the new format, and otherwise starts where core says.
struct SaveFormatSwitchTests {
    @Test
    func aBitrateCarriesBetweenLossyFormatsIntoTheNewRange() {
        #expect(
            BridgeSaveCodec.aac(bitrateKbps: 512).switched(to: .mp3)
                == .mp3(bitrateKbps: 320)
        )
        #expect(
            BridgeSaveCodec.mp3(bitrateKbps: 128).switched(to: .opusOgg)
                == .opusOgg(bitrateKbps: 128)
        )
    }

    @Test
    func aBitDepthCarriesBetweenLosslessFormats() {
        #expect(
            BridgeSaveCodec.flac(bitDepth: .bits16).switched(to: .aiff)
                == .aiff(bitDepth: .bits16)
        )
    }

    @Test
    func crossingBetweenLosslessAndLossyStartsAtCoresCodec() {
        #expect(
            BridgeSaveCodec.flac(bitDepth: .bits16).switched(to: .aac)
                == .aac(bitrateKbps: 256)
        )
        #expect(
            BridgeSaveCodec.opusOgg(bitrateKbps: 96).switched(to: .wav)
                == .wav(bitDepth: .bits24)
        )
    }
}
