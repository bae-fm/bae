package fm.bae.app.ui.appearance

import android.animation.ValueAnimator
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.database.ContentObserver
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.provider.Settings
import androidx.annotation.MainThread
import androidx.annotation.RequiresApi

/** Reports the current platform setting and its changes on the UI thread until the caller unsubscribes. */
@MainThread
internal fun observeAndroidAnimatorScale(
    context: Context,
    onChange: (Float) -> Unit,
): () -> Unit =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        observePlatformScale(onChange)
    } else {
        observeLegacyScale(context, onChange)
    }

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
private fun observePlatformScale(onChange: (Float) -> Unit): () -> Unit {
    val main = Handler(Looper.getMainLooper())
    var active = true
    val callback = Runnable { if (active) onChange(ValueAnimator.getDurationScale()) }
    val listener = ValueAnimator.DurationScaleChangeListener { main.post(callback) }
    ValueAnimator.registerDurationScaleChangeListener(listener)
    callback.run()
    return {
        active = false
        ValueAnimator.unregisterDurationScaleChangeListener(listener)
        main.removeCallbacks(callback)
    }
}

private fun observeLegacyScale(
    context: Context,
    onChange: (Float) -> Unit,
): () -> Unit {
    val resolver = context.contentResolver
    val power = checkNotNull(context.getSystemService(PowerManager::class.java))
    var active = true

    fun report() {
        if (!active) return
        // Android can deliver both signals before WindowManager updates this process's animator scale.
        val scale =
            if (power.isPowerSaveMode) {
                0f
            } else {
                Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f)
            }
        onChange(scale)
    }
    val batterySaver =
        object : BroadcastReceiver() {
            override fun onReceive(
                context: Context,
                intent: Intent,
            ) = report()
        }
    val scaleSetting =
        object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) {
                report()
            }
        }
    // Only Android can send this protected broadcast. This path runs before Android added receiver flags.
    context.registerReceiver(batterySaver, IntentFilter(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED))
    val scaleUri = Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE)
    resolver.registerContentObserver(scaleUri, false, scaleSetting)
    report()
    return {
        active = false
        resolver.unregisterContentObserver(scaleSetting)
        context.unregisterReceiver(batterySaver)
    }
}
