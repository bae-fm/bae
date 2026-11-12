package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import fm.bae.app.R
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidRadio
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.AndroidStorageUsage
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.storageUsageSegments
import uniffi.bae_bridge.BridgeCacheLimit
import uniffi.bae_bridge.BridgeCacheLimitChoice
import uniffi.bae_bridge.BridgeDeviceStorage
import uniffi.bae_bridge.BridgeStorageKind

/** The Storage page over what the phone keeps, and the changes it asks for. */
@Composable
internal fun AndroidSettingsStorage(
    storage: BridgeDeviceStorage,
    asksBeforeUnpinning: Boolean,
    onBack: () -> Unit,
    onClearCache: () -> Unit,
    onKeepUpTo: () -> Unit,
    onAskBeforeUnpinning: (Boolean) -> Unit,
    onDeleteBeforeReset: () -> Unit,
) {
    val format = rememberByteFormat()
    val resources = LocalResources.current
    AndroidScreen(
        title = stringResource(R.string.settings_storage),
        onBack = onBack,
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            AndroidCard {
                AndroidStorageUsage(
                    total = format(storage.total),
                    of = stringResource(R.string.storage_on_this_phone),
                    segments = storageUsageSegments(storage, { resources.getString(it.label) }, format),
                )
            }
            CacheSection(storage, format, onClearCache, onKeepUpTo)
            PinnedSection(asksBeforeUnpinning, onAskBeforeUnpinning)
            storage.beforeReset?.let { BeforeResetSection(format(it), onDeleteBeforeReset) }
        }
    }
}

/** Clear cache and Keep up to, under what the cache holds. */
@Composable
private fun CacheSection(
    storage: BridgeDeviceStorage,
    format: (ULong) -> String,
    onClearCache: () -> Unit,
    onKeepUpTo: () -> Unit,
) {
    AndroidSection(stringResource(R.string.storage_kind_cache))
    AndroidCard {
        AndroidRow(
            title = stringResource(R.string.storage_clear_cache),
            subtitle = format(storage.cache),
            action = true,
            enabled = storage.canClearCache,
            onClick = onClearCache,
        )
        AndroidRow(
            title = stringResource(R.string.storage_keep_up_to),
            subtitle = format(storage.cacheLimit.bytes),
            onClick = onKeepUpTo,
        )
    }
    AndroidNote(stringResource(R.string.storage_cache_note))
}

/** The switch that decides whether Unpin asks first. */
@Composable
private fun PinnedSection(
    asksBeforeUnpinning: Boolean,
    onAskBeforeUnpinning: (Boolean) -> Unit,
) {
    AndroidSection(stringResource(R.string.storage_kind_pinned))
    AndroidCard {
        val askTitle = stringResource(R.string.storage_ask_before_unpinning)
        AndroidRow(
            title = askTitle,
            subtitle = stringResource(R.string.storage_ask_before_unpinning_note),
            trailing = {
                Switch(
                    checked = asksBeforeUnpinning,
                    onCheckedChange = onAskBeforeUnpinning,
                    modifier = Modifier.semantics { contentDescription = askTitle },
                )
            },
        )
    }
    AndroidNote(stringResource(R.string.storage_pinned_note))
}

/** Deleting the [size] a reset kept. */
@Composable
private fun BeforeResetSection(
    size: String,
    onDelete: () -> Unit,
) {
    AndroidSection(stringResource(R.string.storage_kind_before_reset))
    AndroidCard {
        AndroidRow(
            title = stringResource(R.string.storage_delete_before_reset),
            subtitle = stringResource(R.string.storage_before_reset_nothing_uses, size),
            action = true,
            destructive = true,
            onClick = onDelete,
        )
    }
}

/** The name of a kind of what the phone keeps. */
private val BridgeStorageKind.label: Int
    @StringRes
    get() =
        when (this) {
            BridgeStorageKind.PINNED -> R.string.storage_kind_pinned
            BridgeStorageKind.CACHE -> R.string.storage_kind_cache
            BridgeStorageKind.LIBRARY -> R.string.storage_kind_library
            BridgeStorageKind.BEFORE_RESET -> R.string.storage_kind_before_reset
        }

/** A byte count as the phone writes sizes, "1.2 GB". */
@Composable
internal fun rememberByteFormat(): (ULong) -> String {
    val context = LocalContext.current
    return remember(context) { { bytes -> context.formatFileSize(bytes.requireDisplayableByteCount()) } }
}

/** How much audio the cache keeps: a radio row for each of [limits], [selected] checked. */
@Composable
internal fun keepUpToParts(
    limits: List<BridgeCacheLimit>,
    selected: BridgeCacheLimitChoice,
    onSelect: (BridgeCacheLimitChoice) -> Unit,
    onDismiss: () -> Unit,
): DialogParts {
    val format = rememberByteFormat()
    return DialogParts(
        title = stringResource(R.string.storage_keep_up_to),
        content = {
            Column(modifier = Modifier.selectableGroup()) {
                for (limit in limits) {
                    AndroidRadio(
                        label = format(limit.bytes),
                        checked = limit.choice == selected,
                        onClick = { onSelect(limit.choice) },
                    )
                }
            }
        },
        confirmButton = {},
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}

/** The question before deleting the [bytes] the reset kept. */
@Composable
internal fun deleteBeforeResetParts(
    bytes: ULong,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    DialogParts(
        title = stringResource(R.string.storage_delete_before_reset_title),
        message = stringResource(R.string.storage_delete_before_reset_body, rememberByteFormat()(bytes)),
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.DESTRUCTIVE, onClick = onConfirm) {
                Text(stringResource(R.string.storage_delete))
            }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
