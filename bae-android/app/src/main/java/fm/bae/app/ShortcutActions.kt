package fm.bae.app

import android.content.Intent

/**
 * A launcher app shortcut (declared in res/xml/shortcuts.xml). Each shortcut's
 * launch intent carries [extraValue] in [EXTRA_SHORTCUT_ACTION] so the launch
 * that started — or re-entered — MainActivity resolves back to what to do.
 */
enum class ShortcutAction(
    val extraValue: String,
) {
    /** Open the active library and resume playback where it left off. */
    RESUME("resume"),

    /** Open the library with the search field focused. */
    SEARCH("search"),
}

/**
 * Intent extra naming which app shortcut launched MainActivity. The <extra>
 * entries in res/xml/shortcuts.xml set it; MainActivity reads it on cold start
 * (onCreate) and on warm relaunch (onNewIntent).
 */
const val EXTRA_SHORTCUT_ACTION = "fm.bae.app.SHORTCUT_ACTION"

/**
 * Resolve the shortcut a launch [intent] carries, or null for an ordinary
 * launch. Driven off the enum's own [ShortcutAction.extraValue] so the shortcut
 * XML and this mapping share one source of truth: an absent extra is an ordinary
 * launch, and any unrecognized value likewise resolves to no action.
 */
fun shortcutActionFromIntent(intent: Intent?): ShortcutAction? {
    val value = intent?.getStringExtra(EXTRA_SHORTCUT_ACTION)
    return ShortcutAction.entries.firstOrNull { it.extraValue == value }
}
