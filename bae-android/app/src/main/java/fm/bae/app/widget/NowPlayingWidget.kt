package fm.bae.app.widget

import android.content.Context
import android.graphics.drawable.Icon
import androidx.compose.runtime.Composable
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
import androidx.glance.material3.ColorProviders
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import fm.bae.app.R
import fm.bae.app.mainActivityIntent
import fm.bae.app.playback.ArtworkContentProvider
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.appearance.appearanceColorScheme
import fm.bae.app.ui.appearance.appearanceFile
import fm.bae.app.ui.appearance.readAppearancePreferences
import fm.bae.app.ui.appearance.surfaces
import uniffi.bae_bridge.BridgeImageRef
import androidx.compose.ui.text.font.FontWeight as ComposeFontWeight
import androidx.glance.color.ColorProvider as DayNightColorProvider

/**
 * Home-screen now-playing widget, drawn from the file-backed [WidgetSnapshot]
 * because the launcher's process can't host bae-core.
 */
class NowPlayingWidget : GlanceAppWidget() {
    override suspend fun provideGlance(
        context: Context,
        id: GlanceId,
    ) {
        val snapshot = WidgetSnapshotStore(context).read()
        val preferences = readAppearancePreferences(appearanceFile(context))
        provideContent {
            GlanceTheme(colors = widgetColors(preferences)) {
                NowPlayingWidgetContent(snapshot, placeholder = placeholderColor(preferences))
            }
        }
    }
}

@Composable
private fun NowPlayingWidgetContent(
    snapshot: WidgetSnapshot,
    placeholder: ColorProvider,
) {
    val context = LocalContext.current
    val track = snapshot.track
    // The whole surface opens the app; the transport buttons take their own taps.
    Row(
        modifier =
            GlanceModifier
                .fillMaxSize()
                .background(GlanceTheme.colors.surface)
                .padding(ThemeSpace.group)
                .clickable(actionStartActivity(mainActivityIntent(context))),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Cover(track?.coverImage, placeholder)
        Spacer(GlanceModifier.width(ThemeSpace.group))
        Column(modifier = GlanceModifier.defaultWeight()) {
            Text(
                text = track?.title ?: context.getString(R.string.widget_nothing_playing),
                maxLines = 1,
                style = ThemeText.rowTitle.glanceStyle(GlanceTheme.colors.onSurface),
            )
            if (track != null) {
                Text(
                    text = track.artist,
                    maxLines = 1,
                    style = ThemeText.detail.glanceStyle(GlanceTheme.colors.onSurfaceVariant),
                )
            }
        }
        if (track != null) {
            Spacer(GlanceModifier.width(ThemeSpace.related))
            TransportButton(
                iconRes = if (snapshot.isPlaying) R.drawable.ic_widget_pause else R.drawable.ic_widget_play,
                descriptionRes = if (snapshot.isPlaying) R.string.pause else R.string.play,
                command = COMMAND_TOGGLE,
            )
            Spacer(GlanceModifier.width(ThemeSpace.inline))
            TransportButton(
                iconRes = R.drawable.ic_widget_next,
                descriptionRes = R.string.next_track,
                command = COMMAND_NEXT,
            )
        }
    }
}

@Composable
private fun Cover(
    coverImage: BridgeImageRef?,
    placeholder: ColorProvider,
) {
    val context = LocalContext.current
    val coverSize = ThemeSize.barArtwork
    if (coverImage == null) {
        Box(
            modifier =
                GlanceModifier
                    .size(coverSize)
                    .cornerRadius(ThemeRadius.artwork)
                    .background(placeholder),
            contentAlignment = Alignment.Center,
        ) {
            Image(
                provider = ImageProvider(R.drawable.ic_widget_music_note),
                contentDescription = null,
                modifier = GlanceModifier.size(ThemeIcon.large),
                colorFilter = ColorFilter.tint(GlanceTheme.colors.onSurfaceVariant),
            )
        }
    } else {
        // Glance has no Uri ImageProvider, so an Icon carries the artwork URI for ArtworkContentProvider.
        val coverIcon = Icon.createWithContentUri(ArtworkContentProvider.uriFor(context, coverImage))
        Image(
            provider = ImageProvider(coverIcon),
            contentDescription = null,
            modifier = GlanceModifier.size(coverSize).cornerRadius(ThemeRadius.artwork),
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
                .size(ThemeSize.hitTarget)
                .cornerRadius(ThemeSize.hitTarget / 2)
                .clickable(actionRunCallback<NowPlayingWidgetTransportAction>(widgetCommand(command))),
        contentAlignment = Alignment.Center,
    ) {
        Image(
            provider = ImageProvider(iconRes),
            contentDescription = context.getString(descriptionRes),
            modifier = GlanceModifier.size(ThemeIcon.large),
            colorFilter = ColorFilter.tint(GlanceTheme.colors.onSurface),
        )
    }
}

/** The app's colour scheme for the chosen appearance, in light and dark. */
private fun widgetColors(preferences: AppearancePreferences) =
    ColorProviders(
        light = appearanceColorScheme(preferences, dark = preferences.mode == AppearanceMode.DARK),
        dark = appearanceColorScheme(preferences, dark = preferences.mode != AppearanceMode.LIGHT),
    )

/** The chosen tone's placeholder surface, behind a missing cover. */
private fun placeholderColor(preferences: AppearancePreferences): ColorProvider =
    DayNightColorProvider(
        day = preferences.tone.surfaces(dark = preferences.mode == AppearanceMode.DARK).placeholder,
        night = preferences.tone.surfaces(dark = preferences.mode != AppearanceMode.LIGHT).placeholder,
    )

/** A role as a Glance text style, its weight rounded down to Glance's normal, medium or bold. */
private fun ThemeText.glanceStyle(color: ColorProvider): TextStyle {
    val weight = checkNotNull(style.fontWeight) { "every text role sets a weight" }
    return TextStyle(
        color = color,
        fontSize = style.fontSize,
        fontWeight =
            when {
                weight >= ComposeFontWeight.Bold -> FontWeight.Bold
                weight >= ComposeFontWeight.Medium -> FontWeight.Medium
                else -> FontWeight.Normal
            },
    )
}
