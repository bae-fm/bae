package fm.bae.app.ui

import uniffi.bae_bridge.BridgeRepeatMode

/** The next mode in a repeat button's cycle: OFF → CONTEXT → TRACK → OFF.
 *  UI-owned: core only accepts absolute setRepeatMode values; the caller
 *  computes the target from the mode it renders. */
fun BridgeRepeatMode.next(): BridgeRepeatMode =
    when (this) {
        BridgeRepeatMode.OFF -> BridgeRepeatMode.CONTEXT
        BridgeRepeatMode.CONTEXT -> BridgeRepeatMode.TRACK
        BridgeRepeatMode.TRACK -> BridgeRepeatMode.OFF
    }
