package fm.bae.app.widget

import android.content.Context
import android.graphics.Bitmap
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
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeApp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.DecodeSize
import fm.bae.app.data.ImageContent
import fm.bae.app.mainActivityIntent
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
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeImageRef
import kotlin.math.roundToInt
import androidx.compose.ui.text.font.FontWeight as ComposeFontWeight
import androidx.glance.color.ColorProvider as DayNightColorProvider

private val logger = BaeLogger("bae.NowPlayingWidget")

/**
 * Home-screen now-playing widget, drawn from the file-backed [WidgetSnapshot],
 * which a render without an open library can still read, and played or paused
 * by the open library's player when there is one ([widgetContent]).
 */
class NowPlayingWidget : GlanceAppWidget() {
    override suspend fun provideGlance(
        context: Context,
        id: GlanceId,
    ) {
        val read = WidgetSnapshotStore(context).read()
        if (read is WidgetRead.Damaged) {
            (context.applicationContext as BaeApp).bugs.reportShownInPlace("read the widget's saved track", read.error)
        }
        val session = AppSessionHolder.currentSession()
        val content = widgetContent(read, session?.playback)
        val cover = widgetCover(context, (content as? WidgetContent.Track)?.track?.coverImage, session)
        val preferences = readAppearancePreferences(appearanceFile(context))
        provideContent {
            GlanceTheme(colors = widgetColors(preferences)) {
                NowPlayingWidgetContent(content, cover, placeholder = placeholderColor(preferences))
            }
        }
    }
}

/**
 * The now-playing cover decoded at the widget's cover size, or null for the
 * placeholder: no cover, no open library to read it from, or a read that failed
 * (logged). The launcher draws the widget in its own process, so the cover
 * crosses to it as pixels; a `content://` URI of this app's unexported artwork
 * provider is one the launcher cannot read.
 */
private suspend fun widgetCover(
    context: Context,
    image: BridgeImageRef?,
    session: OpenLibrary?,
): Bitmap? {
    val pixels = (ThemeSize.barArtwork.value * context.resources.displayMetrics.density).roundToInt()
    return when {
        image == null -> {
            null
        }

        session == null -> {
            logger.debug("widget cover ${image.id} with no open library to read it from")
            null
        }

        else -> {
            try {
                session.imageStore.image(ImageContent.LibraryImage(image), DecodeSize.FitTo(pixels))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.error("failed to load the widget's cover ${image.id}", e)
                null
            }
        }
    }
}

@Composable
private fun NowPlayingWidgetContent(
    content: WidgetContent,
    cover: Bitmap?,
    placeholder: ColorProvider,
) {
    val context = LocalContext.current
    val track = (content as? WidgetContent.Track)?.track
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
        Cover(cover, placeholder)
        Spacer(GlanceModifier.width(ThemeSpace.group))
        Column(modifier = GlanceModifier.defaultWeight()) {
            Text(
                text =
                    when (content) {
                        is WidgetContent.Track -> content.track.title
                        WidgetContent.NothingPlaying -> context.getString(R.string.widget_nothing_playing)
                        WidgetContent.SomethingWentWrong -> context.coreString("core.error.category.internal")
                    },
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
        if (content is WidgetContent.Track) {
            Spacer(GlanceModifier.width(ThemeSpace.related))
            TransportButton(
                iconRes = if (content.isPlaying) R.drawable.ic_widget_pause else R.drawable.ic_widget_play,
                descriptionRes = if (content.isPlaying) R.string.pause else R.string.play,
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
    cover: Bitmap?,
    placeholder: ColorProvider,
) {
    val coverSize = ThemeSize.barArtwork
    if (cover == null) {
        Spacer(
            modifier =
                GlanceModifier
                    .size(coverSize)
                    .cornerRadius(ThemeRadius.artwork)
                    .background(placeholder),
        )
    } else {
        Image(
            provider = ImageProvider(cover),
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
