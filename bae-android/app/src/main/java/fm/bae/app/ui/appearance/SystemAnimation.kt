package fm.bae.app.ui.appearance

import android.content.Context
import android.provider.Settings

internal fun systemAnimationsEnabled(context: Context): Boolean =
    Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) != 0f
