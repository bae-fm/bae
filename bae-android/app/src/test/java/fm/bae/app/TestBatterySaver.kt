package fm.bae.app

import android.app.Activity
import android.content.Intent
import android.os.PowerManager
import org.robolectric.Shadows.shadowOf

/** Updates Robolectric's model of Battery Saver and delivers Android's notification. */
internal fun setTestBatterySaver(
    activity: Activity,
    enabled: Boolean,
) {
    // The test app must be able to register without AndroidX's app-specific receiver permission.
    shadowOf(activity.application).denyPermissions("${activity.packageName}.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION")
    val power = checkNotNull(activity.getSystemService(PowerManager::class.java))
    shadowOf(power).setIsPowerSaveMode(enabled)
    activity.sendBroadcast(Intent(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED))
}
