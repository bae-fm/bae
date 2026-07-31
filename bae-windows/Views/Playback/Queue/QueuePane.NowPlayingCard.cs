using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.UI;

namespace Bae.Windows;

// The fixed now-playing card between the header and the list: its build and
// its store-driven rendering. Split out of QueuePane.cs unchanged.
internal sealed partial class QueuePane
{
    // The now-playing card: fixed chrome between the header and the list, rendering
    // the same track the bar shows. Not a ListView row — building it here keeps the
    // list's virtualization untouched. Hidden until RenderCard shows it.
    private Border BuildNowPlayingCard()
    {
        _cardCover = new Image { Stretch = Stretch.UniformToFill };
        var art = new Border
        {
            Width = 56,
            Height = 56,
            CornerRadius = new CornerRadius(9),
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = _cardCover,
            Translation = new System.Numerics.Vector3(0, 0, 8),
        };
        art.Shadow = new ThemeShadow();

        var eyebrow = new TextBlock
        {
            Text = Loc.Chrome("queue.now_playing").ToUpper(CultureInfo.CurrentUICulture),
            FontSize = 9,
            FontWeight = FontWeights.Bold,
            CharacterSpacing = 130,
            Foreground = Accent,
        };
        _cardTitle = new TextBlock
        {
            FontSize = 14,
            FontWeight = FontWeights.Bold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        _cardArtist = new TextBlock
        {
            FontSize = 11,
            FontWeight = FontWeights.Medium,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Foreground = Secondary,
        };

        var text = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(eyebrow);
        text.Children.Add(_cardTitle);
        text.Children.Add(_cardArtist);
        text.Children.Add(BuildCardProgress());

        var row = new Grid();
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        Grid.SetColumn(art, 0);
        Grid.SetColumn(text, 1);
        art.Margin = new Thickness(0, 0, 12, 0);
        row.Children.Add(art);
        row.Children.Add(text);

        _card = new Border
        {
            Margin = new Thickness(14, 0, 14, 6),
            Padding = new Thickness(12),
            CornerRadius = new CornerRadius(12),
            BorderThickness = new Thickness(1),
            BorderBrush = ForegroundWash(0.08),
            Background = CardWashBrush(),
            Visibility = Visibility.Collapsed,
            Child = row,
        };
        return _card;
    }

    // The card's progress strip: a display-only 4px track with an accent-gradient
    // fill and a fixed-width elapsed readout. No seek — it mirrors the store's
    // position.
    private Grid BuildCardProgress()
    {
        _cardProgressFill = new ColumnDefinition { Width = new GridLength(0, GridUnitType.Star) };
        _cardProgressRest = new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) };

        var fill = new Border
        {
            CornerRadius = new CornerRadius(2),
            Background = ProgressFillBrush(),
        };
        var trackGrid = new Grid();
        trackGrid.ColumnDefinitions.Add(_cardProgressFill);
        trackGrid.ColumnDefinitions.Add(_cardProgressRest);
        Grid.SetColumn(fill, 0);
        trackGrid.Children.Add(fill);

        var track = new Border
        {
            Height = 4,
            CornerRadius = new CornerRadius(2),
            Background = ForegroundWash(0.12),
            VerticalAlignment = VerticalAlignment.Center,
            Child = trackGrid,
        };

        _cardElapsed = new TextBlock
        {
            Text = "0:00",
            Width = 34,
            TextAlignment = TextAlignment.Right,
            FontSize = 10,
            FontWeight = FontWeights.SemiBold,
            Foreground = Secondary,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var strip = new Grid { Margin = new Thickness(0, 6, 0, 0), ColumnSpacing = 8 };
        strip.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        strip.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(track, 0);
        Grid.SetColumn(_cardElapsed, 1);
        strip.Children.Add(track);
        strip.Children.Add(_cardElapsed);
        return strip;
    }

    // -- Now-playing card rendering ---------------------------------------

    private void OnNowPlayingChanged(NowPlayingBarTrack track)
    {
        _nowPlaying = track;
        RenderCard();
    }

    private void OnPlaybackStopped()
    {
        _nowPlaying = null;
        _position = null;
        RenderCard();
    }

    private void OnPositionChanged(PlaybackPositionRender render)
    {
        _position = render;
        RenderCardPosition(force: false);
    }

    // Render the card from the cached now-playing track: show it with the track's
    // art/title/artist while playback is active, hide it when stopped. No-op while
    // the pane is closed (the card chrome doesn't exist).
    private void RenderCard()
    {
        if (_card is null)
        {
            return;
        }
        if (_nowPlaying is { } track)
        {
            _card.Visibility = Visibility.Visible;
            _cardTitle!.Text = track.Title;
            _cardArtist!.Text = track.Artist;
            _images.Bind(
                _cardCover!,
                ImageContent.ForLibraryImage(track.CoverImage),
                ImageWidths.Row);
            _renderedSecond = -1;
            RenderCardPosition(force: true);
        }
        else
        {
            _card.Visibility = Visibility.Collapsed;
        }
    }

    // Update the progress fill and elapsed readout, collapsed to one update per
    // second (a readout, not a scrubber). `force` re-renders on a track change even
    // within the same second.
    private void RenderCardPosition(bool force)
    {
        if (_card is null || _nowPlaying is null || _position is not { } position)
        {
            return;
        }
        var second = (long)(position.PositionMs / 1000);
        if (!force && second == _renderedSecond)
        {
            return;
        }
        _renderedSecond = second;
        var progress = Math.Clamp(position.Progress, 0, 1);
        _cardProgressFill!.Width = new GridLength(progress, GridUnitType.Star);
        _cardProgressRest!.Width = new GridLength(1 - progress, GridUnitType.Star);
        _cardElapsed!.Text = BridgeDisplay.Clock(position.PositionMs);
    }
}
