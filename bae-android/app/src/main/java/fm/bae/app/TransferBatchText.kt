package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeTransferBatch

/** How many of a transfer line's releases are done, of all its batch holds ("1 of 3 releases"), from core's catalog. */
fun Context.releasesDoneText(batch: BridgeTransferBatch): String =
    coreString("core.activity.releases_done", mapOf("done" to batch.done.toInt(), "total" to batch.total.toInt()))
