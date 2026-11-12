package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.currentLocale
import fm.bae.app.spokenListText
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.appearance.colors
import uniffi.bae_bridge.BridgeDeviceStorage
import uniffi.bae_bridge.BridgeStorageKind

/** One kind of what the phone keeps, as the bar and legend draw it: [share] of the bar, [size] in the legend. */
internal data class StorageUsageSegment(
    val kind: BridgeStorageKind,
    val label: String,
    val size: String,
    val share: Float,
)

/**
 * The segments for [storage]'s parts, in the order core sends them, each sized in proportion to its bytes over
 * the total. [label] names a kind and [size] writes a byte count.
 */
internal fun storageUsageSegments(
    storage: BridgeDeviceStorage,
    label: (BridgeStorageKind) -> String,
    size: (ULong) -> String,
): List<StorageUsageSegment> =
    storage.parts.map { part ->
        StorageUsageSegment(
            kind = part.kind,
            label = label(part.kind),
            size = size(part.bytes),
            share = if (storage.total == 0uL) 0f else (part.bytes.toDouble() / storage.total.toDouble()).toFloat(),
        )
    }

/**
 * The accent each kind keeps whatever its size and whichever kinds show: Pinned blue, Cache gold, Library orchid,
 * From before the reset mint. The same in both appearances.
 */
internal val BridgeStorageKind.accent: AccentChoice
    get() =
        when (this) {
            BridgeStorageKind.PINNED -> AccentChoice.BLUE
            BridgeStorageKind.CACHE -> AccentChoice.GOLD
            BridgeStorageKind.LIBRARY -> AccentChoice.ORCHID
            BridgeStorageKind.BEFORE_RESET -> AccentChoice.MINT
        }

/** The colour a kind is drawn in, its accent's selection step. */
internal val BridgeStorageKind.color: Color
    get() = accent.colors.selection

/**
 * What the phone keeps, as Settings → Storage heads it: the [total] with [of] after it, one bar split by kind, and a
 * legend line per kind with its size. The legend names every kind, so no colour stands alone.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun AndroidStorageUsage(
    total: String,
    of: String,
    segments: List<StorageUsageSegment>,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier =
            modifier
                .fillMaxWidth()
                .padding(horizontal = ThemeSpace.edge)
                .padding(top = ThemeSpace.edge, bottom = ThemeSpace.group),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
            Text(
                text = total,
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.alignByBaseline(),
            )
            Text(
                text = of,
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.alignByBaseline(),
            )
        }
        UsageBar(segments)
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
        ) {
            for (segment in segments) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(modifier = Modifier.size(DOT).background(segment.kind.color, CircleShape))
                    Text(
                        text = segment.label,
                        style = ThemeText.body.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(
                        text = segment.size,
                        style = ThemeText.body.style,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
            }
        }
    }
}

/**
 * The bar, one rounded piece per kind in proportion to its share; a thin kind still shows, at least
 * [MIN_SEGMENT] wide, and the others give up that width in proportion.
 */
@Composable
private fun UsageBar(segments: List<StorageUsageSegment>) {
    val description =
        spokenListText(segments.map { "${it.label} ${it.size}" }, LocalContext.current.currentLocale())
    BoxWithConstraints(modifier = Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = description }) {
        val widths = segmentWidths(segments.map { it.share }, maxWidth - BAR_GAP * (segments.size - 1))
        Row(modifier = Modifier.height(BAR_HEIGHT), horizontalArrangement = Arrangement.spacedBy(BAR_GAP)) {
            segments.forEachIndexed { index, segment ->
                val start = if (index == 0) END_RADIUS else INNER_RADIUS
                val end = if (index == segments.lastIndex) END_RADIUS else INNER_RADIUS
                Box(
                    modifier =
                        Modifier
                            .width(widths[index])
                            .height(BAR_HEIGHT)
                            .background(segment.kind.color, segmentShape(start, end)),
                )
            }
        }
    }
}

private fun segmentShape(
    start: Dp,
    end: Dp,
) = RoundedCornerShape(topStart = start, topEnd = end, bottomEnd = end, bottomStart = start)

/** Each share's width of [available], none under [MIN_SEGMENT]: the thin ones are raised to it, the rest shrink. */
internal fun segmentWidths(
    shares: List<Float>,
    available: Dp,
): List<Dp> {
    val total = shares.sum()
    val thin = shares.map { total <= 0f || it / total * available.value < MIN_SEGMENT.value }
    val restShare = shares.filterIndexed { i, _ -> !thin[i] }.sum()
    val room = (available - MIN_SEGMENT * thin.count { it }).value
    return shares.mapIndexed { i, share ->
        if (thin[i] || restShare <= 0f) MIN_SEGMENT else (share / restShare * room).dp
    }
}

private val BAR_HEIGHT = 8.dp
private val BAR_GAP = 2.dp
private val END_RADIUS = 4.dp
private val INNER_RADIUS = 2.dp
private val DOT = 8.dp

/** The least of the bar a kind with bytes takes, so it stays visible. */
private val MIN_SEGMENT = 4.dp
