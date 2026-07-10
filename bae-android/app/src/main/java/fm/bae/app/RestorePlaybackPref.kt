package fm.bae.app

import android.content.Context

/**
 * The device-local "Restore on launch" preference. Default on: the app resumes
 * where playback left off unless the user turns it off. Read at the next
 * `initApp` (library open) — the core keeps the resume row current either way,
 * so flipping it on takes effect at the next launch.
 */
object RestorePlaybackPref {
    private const val PREFS = "playback"
    private const val KEY = "restore_on_launch"

    fun load(context: Context): Boolean = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY, true)

    fun save(
        context: Context,
        enabled: Boolean,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY, enabled)
            .apply()
    }
}
