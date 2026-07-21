using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using uniffi.bae_bridge;
using Windows.UI;

namespace Bae.Windows;

// Renders the now-playing bar from PlaybackStore change events: track title /
// artist / cover / transport glyph, the seek bar and its labels, the volume and
// mute controls, and the transient "+N" queue-add badge. Owns the drag state of
// the seek slider (so live progress events don't fight the drag) and the volume
// suppression flag (so programmatic volume changes don't echo back as a set).
internal sealed class NowPlayingBarController
{
    private readonly SessionStore _session;
    private readonly PlaybackStore _playback;
    private readonly Func<XamlRoot?> _xamlRoot;

    private readonly Border _nowPlayingBar;
    private readonly Image _cover;
    private readonly TextBlock _title;
    private readonly TextBlock _artist;
    private readonly TextBlock _elapsed;
    private readonly TextBlock _duration;
    private readonly Slider _progress;
    private readonly Slider _volume;
    private readonly Button _playPause;
    private readonly FontIcon _playPauseGlyph;
    private readonly Button _mute;
    private readonly FontIcon _muteGlyph;
    private readonly Button _repeat;
    private readonly FontIcon _repeatGlyph;
    private readonly Button _shuffle;
    private readonly FontIcon _shuffleGlyph;
    private readonly Button _previous;
    private readonly Button _next;
    private readonly ProgressRing _loading;
    private readonly Border _queueAddBadge;
    private readonly ScaleTransform _queueAddBadgeScale;
    private readonly TextBlock _queueAddBadgeText;

    // Accent tint for active shuffle/repeat glyphs and their soft fill; the
    // neutral tint for inactive ones. Resolved once from the app resources, the
    // same brushes the queue pane's shuffle toggle uses.
    private readonly Brush _accentBrush;
    private readonly Brush _secondaryBrush;
    private readonly Brush _accentSoftFill;

    // True while the user is dragging the seek slider, so progress events don't
    // fight the drag; the seek is set on release.
    private bool _userSeeking;

    // Suppresses the volume slider's ValueChanged while it's set programmatically
    // (seeding + VolumeChanged events), so it doesn't echo back as a SetVolume.
    private bool _suppressVolume;

    // Holds the +N queue badge visible for ~1.4s after the last add; a fresh add
    // restarts it, replacing the count and resetting the timer.
    private DispatcherTimer? _queueBadgeTimer;

    // Whether the leading time label shows a remaining countdown instead of
    // elapsed time. Read from the config mirror, never stored here: it is a synced
    // preference, so it follows the user to every device. Clicking the label writes
    // the config; the config invalidation calls RefreshTimeLabelMode, which is what
    // re-renders the bar.
    private readonly Func<bool> _showRemainingTime;

    private bool ShowRemaining => _showRemainingTime();

    // The last rendered position, so a label toggle re-renders immediately from
    // it instead of waiting for the next progress tick. Cleared when the bar
    // hides, so a stale position can't resurrect via a later toggle.
    private PlaybackPositionRender? _lastPosition;

    public NowPlayingBarController(
        SessionStore session,
        PlaybackStore playback,
        Func<bool> showRemainingTime,
        Func<XamlRoot?> xamlRoot,
        Border nowPlayingBar,
        Image cover,
        TextBlock title,
        TextBlock artist,
        TextBlock elapsed,
        TextBlock duration,
        Slider progress,
        Slider volume,
        Button playPause,
        FontIcon playPauseGlyph,
        Button mute,
        FontIcon muteGlyph,
        Button repeat,
        FontIcon repeatGlyph,
        Button shuffle,
        FontIcon shuffleGlyph,
        Button previous,
        Button next,
        ProgressRing loading,
        Border queueAddBadge,
        ScaleTransform queueAddBadgeScale,
        TextBlock queueAddBadgeText)
    {
        _session = session;
        _playback = playback;
        _showRemainingTime = showRemainingTime;
        _xamlRoot = xamlRoot;
        _nowPlayingBar = nowPlayingBar;
        _cover = cover;
        _title = title;
        _artist = artist;
        _elapsed = elapsed;
        _duration = duration;
        _progress = progress;
        _volume = volume;
        _playPause = playPause;
        _playPauseGlyph = playPauseGlyph;
        _mute = mute;
        _muteGlyph = muteGlyph;
        _repeat = repeat;
        _repeatGlyph = repeatGlyph;
        _shuffle = shuffle;
        _shuffleGlyph = shuffleGlyph;
        _previous = previous;
        _next = next;
        _loading = loading;
        _queueAddBadge = queueAddBadge;
        _queueAddBadgeScale = queueAddBadgeScale;
        _queueAddBadgeText = queueAddBadgeText;

        _accentBrush = (Brush)Application.Current.Resources["AccentTextFillColorPrimaryBrush"];
        _secondaryBrush = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];
        _accentSoftFill = new SolidColorBrush((Color)Application.Current.Resources["SystemAccentColor"]) { Opacity = 0.16 };

        // Seek on release, not on every drag tick. The Slider handles pointer
        // events internally, so register with handledEventsToo.
        _progress.AddHandler(UIElement.PointerPressedEvent,
            new PointerEventHandler((_, _) =>
            {
                _userSeeking = true;
                _playback.ClearSeekProjection();
            }), true);
        _progress.AddHandler(UIElement.PointerReleasedEvent,
            new PointerEventHandler((_, _) =>
            {
                _userSeeking = false;
                if (_session.CurrentHandleOrNull() != null)
                {
                    var projection = _playback.ProjectSeek(_progress.Value, _progress.Minimum, _progress.Maximum);
                    if (projection is not null)
                    {
                        RenderSeekPosition(projection.Progress, projection.TargetPositionMs, projection.DurationMs);
                    }
                    _session.WithCurrentHandle(handle => NativeBae.SeekByRatio(handle, _progress.Value));
                }
            }), true);

        // The leading label click-toggles between elapsed and remaining. The
        // tooltip is the affordance that it's clickable, and it names the mode a
        // click switches to, so it tracks the current state.
        _elapsed.Tapped += (_, _) => ToggleTimeLabel();
        ToolTipService.SetToolTip(_elapsed, Loc.Chrome(PlaybackPositionModel.TimeLabelTooltipKey(ShowRemaining)));

        _playback.NowPlayingChanged += OnNowPlayingChanged;
        _playback.PlaybackStopped += OnPlaybackStopped;
        _playback.LoadingStarted += OnLoadingStarted;
        _playback.PositionChanged += OnPositionChanged;
        _playback.VolumeChanged += OnVolumeChanged;
        _playback.MuteChanged += OnMuteChanged;
        _playback.RepeatChanged += OnRepeatChanged;
        _playback.TransportChanged += OnTransportChanged;
        _playback.QueueItemsAdded += OnQueueItemsAdded;
        // Shuffle reflects the playing context's shuffled flag, delivered by the
        // queue snapshot; disabled when there is no context to shuffle.
        _playback.QueueChanged += RenderShuffle;
    }

    // Seed the volume slider from the current handle without echoing a SetVolume.
    public void SeedVolume()
    {
        _suppressVolume = true;
        var (current, volume) = _session.WithCurrentHandle(handle => NativeBae.GetVolume(handle));
        if (current)
        {
            _volume.Value = volume;
        }
        _suppressVolume = false;
        RenderMuteIcon();
    }

    // The volume slider moved. Ignore programmatic changes; forward user changes
    // to core.
    public void HandleVolumeSliderChanged()
    {
        if (!_suppressVolume && _session.CurrentHandleOrNull() != null)
        {
            _session.WithCurrentHandle(handle => NativeBae.SetVolume(handle, (float)_volume.Value));
        }
    }

    public void Reset()
    {
        _nowPlayingBar.Visibility = Visibility.Collapsed;
        _userSeeking = false;
        _lastPosition = null;
        RenderShuffle();
    }

    private void OnNowPlayingChanged(NowPlayingBarTrack track)
    {
        _nowPlayingBar.Visibility = Visibility.Visible;
        _title.Text = track.Title;
        _artist.Text = track.Artist;
        _playPauseGlyph.Glyph = track.IsPlaying ? "\uE769" : "\uE768";
        // The name reads the action a press performs: pause while playing, play
        // while paused.
        var playPauseName = Loc.Chrome(track.IsPlaying ? "nowplaying.pause" : "action.play");
        AutomationProperties.SetName(_playPause, playPauseName);
        ToolTipService.SetToolTip(_playPause, playPauseName);
        _cover.Source = CoverImage.LoadImage(_session.CurrentHandleOrNull(), track.CoverImageId);
        // Audio is flowing: drop the buffering spinner, restore the play/pause
        // glyph. The circle itself stays put through the swap.
        _loading.IsActive = false;
        _loading.Visibility = Visibility.Collapsed;
        _playPauseGlyph.Visibility = Visibility.Visible;
        if (track.PauseReason is { } reason)
        {
            _ = ShowSidePauseDialog(reason);
        }
    }

    private void OnPlaybackStopped()
    {
        _nowPlayingBar.Visibility = Visibility.Collapsed;
        _userSeeking = false;
        _lastPosition = null;
        _loading.IsActive = false;
        _loading.Visibility = Visibility.Collapsed;
        _playPauseGlyph.Visibility = Visibility.Visible;
    }

    private void OnLoadingStarted()
    {
        // Core is preparing or buffering the track (initial load, or a seek to a
        // position not yet downloaded). Show the bar with a spinner over the play
        // circle; the circle stays and only its glyph hides, so the ring reads
        // against the fill and neighbors don't shift. The prior track's
        // title/cover stay until PlaybackPlaying lands.
        _nowPlayingBar.Visibility = Visibility.Visible;
        _playPauseGlyph.Visibility = Visibility.Collapsed;
        _loading.IsActive = true;
        _loading.Visibility = Visibility.Visible;
    }

    private void OnPositionChanged(PlaybackPositionRender render)
    {
        if (!_userSeeking)
        {
            _progress.Value = render.Progress;
        }
        RenderTimeLabels(render);
    }

    private void RenderSeekPosition(double progress, ulong positionMs, ulong durationMs)
    {
        _progress.Value = progress;
        RenderTimeLabels(new PlaybackPositionRender(progress, positionMs, durationMs));
    }

    // Write both time labels. Core projects the pair from the position, the
    // duration, and the preference — which label is which is its decision, not
    // this bar's. Remembers the position so a preference change re-renders from it.
    private void RenderTimeLabels(PlaybackPositionRender render)
    {
        _lastPosition = render;
        var clocks = BridgeDisplay.SeekBarClocks(
            render.PositionMs, render.DurationMs, ShowRemaining);
        _elapsed.Text = clocks.Leading;
        _duration.Text = clocks.Trailing;
    }

    // Ask core to flip the preference. Nothing is flipped locally: the write fires
    // a config invalidation, which lands back in RefreshTimeLabelMode.
    private void ToggleTimeLabel() =>
        _session.WithCurrentHandle(
            handle => NativeBae.SetShowRemainingTime(handle, !ShowRemaining));

    // The preference changed (here, or on another device). Re-render the tooltip
    // and the labels from the last position, so the bar changes without waiting
    // for the next progress tick.
    public void RefreshTimeLabelMode()
    {
        ToolTipService.SetToolTip(
            _elapsed, Loc.Chrome(PlaybackPositionModel.TimeLabelTooltipKey(ShowRemaining)));
        if (_lastPosition is { } position)
        {
            RenderTimeLabels(position);
        }
    }

    private void OnVolumeChanged(double volume)
    {
        _suppressVolume = true;
        _volume.Value = volume;
        _suppressVolume = false;
        RenderMuteIcon();
    }

    private void OnMuteChanged(bool isMuted)
    {
        RenderMuteIcon();
    }

    // The mute glyph reflects the rendered level: a slashed speaker when muted or
    // at zero, one wave up to the mid threshold, two waves above it. The name
    // reads the action a press performs: unmute while muted, mute otherwise.
    private void RenderMuteIcon()
    {
        _muteGlyph.Glyph = (_playback.IsMuted || _volume.Value <= 0) switch
        {
            true => "\uE74F",
            false when _volume.Value <= 0.55 => "\uE993",
            false => "\uE994",
        };
        var name = Loc.Chrome(_playback.IsMuted ? "nowplaying.unmute" : "nowplaying.mute");
        AutomationProperties.SetName(_mute, name);
        ToolTipService.SetToolTip(_mute, name);
    }

    // Shuffle mirrors the playing context's shuffled flag: accent glyph over a
    // soft accent fill when on, neutral when off, disabled when nothing is
    // playing from a context to shuffle.
    private void RenderShuffle()
    {
        var context = _playback.Context;
        var shuffled = context?.Shuffled ?? false;
        _shuffle.IsEnabled = context is not null;
        _shuffleGlyph.Foreground = shuffled ? _accentBrush : _secondaryBrush;
        _shuffle.Background = shuffled ? _accentSoftFill : new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        var key = shuffled ? "queue.shuffle.off" : "queue.shuffle.on";
        ToolTipService.SetToolTip(_shuffle, Loc.Chrome(key));
        AutomationProperties.SetName(_shuffle, Loc.Chrome(key));
    }

    // Repeat cycles glyph and tint together: repeat-one and repeat-all both tint
    // to the accent when active, off shows the repeat-all glyph in neutral. The
    // name reads the current mode so Narrator announces the state, not just a
    // press target.
    private void OnRepeatChanged(BridgeRepeatMode mode)
    {
        _repeatGlyph.Glyph = mode == BridgeRepeatMode.Track ? "\uE8ED" : "\uE8EE";
        _repeatGlyph.Foreground = mode == BridgeRepeatMode.Off ? _secondaryBrush : _accentBrush;
        _repeat.Background = mode == BridgeRepeatMode.Off
            ? new SolidColorBrush(Microsoft.UI.Colors.Transparent)
            : _accentSoftFill;
        var name = Loc.Chrome(mode switch
        {
            BridgeRepeatMode.Track => "nowplaying.repeat_track",
            BridgeRepeatMode.Context => "nowplaying.repeat_context",
            _ => "nowplaying.repeat_off",
        });
        AutomationProperties.SetName(_repeat, name);
        ToolTipService.SetToolTip(_repeat, name);
    }

    private void OnTransportChanged(bool hasPrevious, bool hasNext)
    {
        _previous.IsEnabled = hasPrevious;
        _next.IsEnabled = hasNext;
    }

    private void OnQueueItemsAdded(int count)
    {
        FlashQueueAddBadge(count);
    }

    // Springs the +N badge in over the queue button, holds it ~1.4s, then fades
    // it out. A fresh add while it's visible replaces the count and restarts the
    // hold timer instead of re-springing.
    private void FlashQueueAddBadge(int count)
    {
        _queueAddBadgeText.Text = $"+{count}";

        var springIn = new Storyboard();

        var fadeIn = new DoubleAnimation
        {
            To = 1.0,
            Duration = new Duration(TimeSpan.FromMilliseconds(150)),
            EnableDependentAnimation = true,
        };
        Storyboard.SetTarget(fadeIn, _queueAddBadge);
        Storyboard.SetTargetProperty(fadeIn, "Opacity");
        springIn.Children.Add(fadeIn);

        foreach (var axis in new[] { "ScaleX", "ScaleY" })
        {
            var scaleUp = new DoubleAnimation
            {
                To = 1.0,
                Duration = new Duration(TimeSpan.FromMilliseconds(250)),
                EasingFunction = new BackEase { EasingMode = EasingMode.EaseOut, Amplitude = 0.4 },
                EnableDependentAnimation = true,
            };
            Storyboard.SetTarget(scaleUp, _queueAddBadgeScale);
            Storyboard.SetTargetProperty(scaleUp, axis);
            springIn.Children.Add(scaleUp);
        }

        springIn.Begin();

        _queueBadgeTimer?.Stop();
        _queueBadgeTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(1400) };
        _queueBadgeTimer.Tick += (_, _) =>
        {
            _queueBadgeTimer?.Stop();

            var fadeOut = new DoubleAnimation
            {
                To = 0.0,
                Duration = new Duration(TimeSpan.FromMilliseconds(250)),
                EnableDependentAnimation = true,
            };
            Storyboard.SetTarget(fadeOut, _queueAddBadge);
            Storyboard.SetTargetProperty(fadeOut, "Opacity");

            var hide = new Storyboard();
            hide.Children.Add(fadeOut);
            hide.Begin();
        };
        _queueBadgeTimer.Start();
    }

    private async System.Threading.Tasks.Task ShowSidePauseDialog(BridgePlaybackPauseReason reason)
    {
        if (reason is not BridgePlaybackPauseReason.SideEnded side)
        {
            return;
        }

        var title = Loc.Core(side.Prompt.TitleKey, "letter", side.Prompt.SideLetter);
        var message = Loc.Core(side.Prompt.MessageKey);
        var dialog = new ContentDialog
        {
            Title = title,
            Content = new TextBlock
            {
                Text = message,
                TextWrapping = TextWrapping.Wrap,
            },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };
        try
        {
            await dialog.ShowAsync();
        }
        catch (Exception ex)
        {
            BaeDiagnostics.Logger.Warning("Failed to show side-pause dialog", ex);
        }
    }
}
