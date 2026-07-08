package fm.bae.app

import android.content.Intent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The launcher shortcuts (res/xml/shortcuts.xml) carry their identity in an
 * intent extra that MainActivity resolves back to a [ShortcutAction]. This pins
 * that mapping: each declared value resolves to its action, and any other launch
 * (no extra, or an unrecognized value) resolves to no action so an ordinary
 * launch never triggers a shortcut side effect.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ShortcutActionsTest {
    private fun intentWith(value: String?): Intent = Intent().apply { value?.let { putExtra(EXTRA_SHORTCUT_ACTION, it) } }

    @Test
    fun resumeExtraMapsToResume() {
        assertEquals(ShortcutAction.RESUME, shortcutActionFromIntent(intentWith("resume")))
    }

    @Test
    fun searchExtraMapsToSearch() {
        assertEquals(ShortcutAction.SEARCH, shortcutActionFromIntent(intentWith("search")))
    }

    @Test
    fun ordinaryLaunchHasNoAction() {
        assertNull(shortcutActionFromIntent(intentWith(null)))
        assertNull(shortcutActionFromIntent(null))
    }

    @Test
    fun unrecognizedExtraHasNoAction() {
        assertNull(shortcutActionFromIntent(intentWith("shuffle-everything")))
        assertNull(shortcutActionFromIntent(intentWith("")))
    }

    @Test
    fun everyActionRoundTripsThroughItsExtraValue() {
        // The <extra> values in res/xml/shortcuts.xml are ShortcutAction.extraValue,
        // so each action's declared value must resolve back to that same action.
        ShortcutAction.entries.forEach { action ->
            assertEquals(action, shortcutActionFromIntent(intentWith(action.extraValue)))
        }
    }
}
