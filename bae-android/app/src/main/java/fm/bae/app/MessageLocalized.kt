package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeMessage

// Renders a catalog message core composed for the current locale. Mirrors
// macOS's and iOS's `BridgeMessage+Localized`.

/** The message in the person's language, its arguments formatted in by name. */
fun Context.coreString(message: BridgeMessage): String {
    val args = message.args.associate { it.name to it.value }
    return coreString(message.key, args)
}
