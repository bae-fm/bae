package fm.bae.app

import android.content.Context
import android.content.Intent
import android.os.PowerManager
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf

/** Updates Robolectric's model of Battery Saver and delivers Android's notification. */
internal fun setTestBatterySaver(
    context: Context,
    enabled: Boolean,
) {
    // Android's broadcast sender does not hold the app's private receiver permission.
    val app = RuntimeEnvironment.getApplication()
    shadowOf(app).denyPermissions("${context.packageName}.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION")
    val power = checkNotNull(context.getSystemService(PowerManager::class.java))
    shadowOf(power).setIsPowerSaveMode(enabled)
    context.sendBroadcast(Intent(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED))
}
