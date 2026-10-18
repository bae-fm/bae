package fm.bae.app

import android.content.Context
import android.os.Build
import android.provider.Settings

private val deviceNameLogger = BaeLogger("bae.DeviceName")

/**
 * The name other members see for this device: the one the person gave it in
 * system settings, or its maker and model when the system keeps none.
 */
internal fun deviceName(context: Context): String {
    val named = Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)
    if (!named.isNullOrBlank()) return named
    val model = "${Build.MANUFACTURER} ${Build.MODEL}"
    deviceNameLogger.debug("the system keeps no device name; naming this device $model")
    return model
}
