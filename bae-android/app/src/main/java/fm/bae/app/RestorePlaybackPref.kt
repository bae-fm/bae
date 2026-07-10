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

    fun load(context: Context): Boolean = prefs(context).getBoolean(KEY, true)

    fun save(
        context: Context,
        enabled: Boolean,
    ) {
        prefs(context).edit().putBoolean(KEY, enabled).apply()
    }

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
