package fm.bae.app.playback

import uniffi.bae_bridge.BridgeDiagnosticsInterface
import uniffi.bae_bridge.BridgeTelemetryEvent

/** The tests' diagnostics: a telemetry event goes nowhere. */
internal class FakeDiagnostics : BridgeDiagnosticsInterface {
    override fun event(event: BridgeTelemetryEvent) {}
}
