package io.crates.keyring

import android.content.Context

class Keyring {
    companion object {
        init {
            System.loadLibrary("bae_bridge")
        }

        external fun initializeNdkContext(context: Context)
    }
}
