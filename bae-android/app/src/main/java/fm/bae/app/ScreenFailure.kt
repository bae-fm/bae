package fm.bae.app

import uniffi.bae_bridge.BridgeException

/**
 * The step of bringing a library up that failed. A failure screen titles the
 * failure by its step and says what to do next, through shared `ui.load_error`
 * catalog keys; a reset-offered failure has no advice of its own, since the
 * reset screen it lands on is the way out.
 */
enum class FailedStep(
    val titleKey: String,
    val adviceKey: String?,
) {
    /** Starting the TLS stack, bae's directory or the host at launch. */
    STARTUP("ui.load_error.startup.title", "ui.load_error.startup.advice"),

    /** Opening the keystore every library's keys live in. */
    SECURE_STORAGE("ui.load_error.secure_storage.title", "ui.load_error.secure_storage.advice"),

    /** Reading which libraries are on this device. */
    LIBRARY_LIST("ui.load_error.library_list.title", "ui.load_error.library_list.advice"),

    /** Opening a library. */
    LIBRARY_OPEN("ui.load_error.library_open.title", "ui.load_error.library_open.advice"),

    /** Opening a library found this device's copy unusable. */
    RESET_OFFERED("ui.load_error.reset_offered.title", null),

    /** Resetting this device's copy of a library. */
    DEVICE_RESET("ui.load_error.device_reset.title", "ui.load_error.device_reset.advice"),
}

/**
 * A failure a screen shows: the step that failed and its error, none when
 * core says the failure has nothing to show — a launch step that was
 * cancelled still stopped the launch.
 */
data class ScreenFailure(
    val step: FailedStep,
    val error: DisplayError?,
)

/** A bridge launch step that stopped the app, and its error. */
data class StartupError(
    val step: FailedStep,
    val error: BridgeException,
)
