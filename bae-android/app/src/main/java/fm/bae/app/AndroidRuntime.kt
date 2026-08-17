package fm.bae.app

import android.content.Context

object AndroidRuntime {
    init {
        System.loadLibrary("bae_bridge")
    }

    private external fun initializeTls(context: Context): String?

    fun initialize(context: Context) {
        initializeTls(context.applicationContext)?.let { detail ->
            error("Failed to initialize Android certificate verification: $detail")
        }
    }
}
