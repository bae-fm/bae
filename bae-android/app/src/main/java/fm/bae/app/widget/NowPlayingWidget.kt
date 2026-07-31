package fm.bae.app.widget

import android.content.Context
import android.graphics.drawable.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.ContentScale
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import fm.bae.app.R
import fm.bae.app.mainActivityIntent
import fm.bae.app.playback.ArtworkContentProvider
import uniffi.bae_bridge.BridgeImageRef

/**
 * Home-screen widget showing the last now-playing state with play/pause and next
 * controls. It renders from the file-backed [WidgetSnapshot] (the launcher's
 * process can't host bae-core), so it shows the last known track even when the
 * app process is dead. Every tap either drives live playback (see
 * [NowPlayingWidgetTransportAction]) or opens the app.
 */
class NowPlayingWidget : GlanceAppWidget() {
    override suspend fun provideGlance(
        context: Context,
        id: GlanceId,
    ) {
        val snapshot = WidgetSnapshotStore(context).read()
        provideContent {
            GlanceTheme {
                NowPlayingWidgetContent(snapshot)
            }
        }
    }
}

@Composable
private fun NowPlayingWidgetContent(snapshot: WidgetSnapshot) {
    val context = LocalContext.current
    val track = snapshot.track
    // The whole surface deep-links into the app; the transport buttons override
    // this within their own bounds.
    Row(
        modifier =
            GlanceModifier
                .fillMaxSize()
                .background(GlanceTheme.colors.surface)
                .padding(12.dp)
                .clickable(actionStartActivity(mainActivityIntent(context))),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Cover(track?.coverImage)
        Spacer(GlanceModifier.width(12.dp))
        Column(modifier = GlanceModifier.defaultWeight()) {
            Text(
                text = track?.title ?: context.getString(R.string.widget_nothing_playing),
                maxLines = 1,
                style =
                    TextStyle(
                        color = GlanceTheme.colors.onSurface,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Medium,
                    ),
            )
            if (track != null) {
                Text(
                    text = track.artist,
                    maxLines = 1,
                    style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant, fontSize = 13.sp),
                )
            }
        }
        if (track != null) {
            Spacer(GlanceModifier.width(8.dp))
            TransportButton(
                iconRes = if (snapshot.isPlaying) R.drawable.ic_widget_pause else R.drawable.ic_widget_play,
                descriptionRes = if (snapshot.isPlaying) R.string.pause else R.string.play,
                command = COMMAND_TOGGLE,
            )
            Spacer(GlanceModifier.width(4.dp))
            TransportButton(
                iconRes = R.drawable.ic_widget_next,
                descriptionRes = R.string.next_track,
                command = COMMAND_NEXT,
            )
        }
    }
}

@Composable
private fun Cover(coverImage: BridgeImageRef?) {
    val context = LocalContext.current
    val coverSize = 56.dp
    if (coverImage == null) {
        Box(
            modifier =
                GlanceModifier
                    .size(coverSize)
                    .cornerRadius(8.dp)
                    .background(GlanceTheme.colors.secondaryContainer),
            contentAlignment = Alignment.Center,
        ) {
            Image(
                provider = ImageProvider(R.drawable.ic_widget_music_note),
                contentDescription = null,
                modifier = GlanceModifier.size(28.dp),
                colorFilter = ColorFilter.tint(GlanceTheme.colors.onSecondaryContainer),
            )
        }
    } else {
        // Glance has no Uri ImageProvider; wrap the artwork content:// URI in an
        // Icon so the launcher resolves the bytes from ArtworkContentProvider
        // (the same path the media browse clients read covers through).
        val coverIcon = Icon.createWithContentUri(ArtworkContentProvider.uriFor(context, coverImage))
        Image(
            provider = ImageProvider(coverIcon),
            contentDescription = null,
            modifier = GlanceModifier.size(coverSize).cornerRadius(8.dp),
            contentScale = ContentScale.Crop,
        )
    }
}

@Composable
private fun TransportButton(
    iconRes: Int,
    descriptionRes: Int,
    command: String,
) {
    val context = LocalContext.current
    Box(
        modifier =
            GlanceModifier
                .size(44.dp)
                .cornerRadius(22.dp)
                .clickable(actionRunCallback<NowPlayingWidgetTransportAction>(widgetCommand(command))),
        contentAlignment = Alignment.Center,
    ) {
        Image(
            provider = ImageProvider(iconRes),
            contentDescription = context.getString(descriptionRes),
            modifier = GlanceModifier.size(26.dp),
            colorFilter = ColorFilter.tint(GlanceTheme.colors.onSurface),
        )
    }
}
