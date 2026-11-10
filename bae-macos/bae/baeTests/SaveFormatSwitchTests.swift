import BaeKit
import Testing

@testable import bae

/// Switching a preset's format keeps the setting that still applies, as core
/// carries it to the new format, and otherwise starts where core says.
struct SaveFormatSwitchTests {
    @Test
    func aBitrateCarriesBetweenLossyFormatsToTheNearestTheNewOneWrites() {
        #expect(
            BridgeSaveCodec.mp3(bitrateKbps: 40).switched(to: .aac)
                == .aac(bitrateKbps: 96)
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
                == .wav(bitDepth: .sameAsSource)
        )
    }
}
