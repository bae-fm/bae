package fm.bae.app.ui

import uniffi.bae_bridge.BridgeRecoveryLines

/** The recovery screens' words as the bridge hands them, for tests that can't load the bridge. */
internal val testRecoveryLines =
    BridgeRecoveryLines(
        deviceResetRequired = "core.error.device_reset_required",
        resetUnfinished = "core.error.replacement_unfinished.reset",
        restoreUnfinished = "core.error.replacement_unfinished.restore",
        deletionUnfinished = "core.error.deletion_unfinished",
        resetWarning = "core.sync.reset_device.warning",
    )
