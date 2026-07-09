using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using uniffi.bae_bridge;

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
    private readonly Button _mute;
    private readonly Button _repeat;
    private readonly Button _previous;
    private readonly Button _next;
    private readonly ProgressRing _loading;
    private readonly Border _queueAddBadge;
    private readonly ScaleTransform _queueAddBadgeScale;
    private readonly TextBlock _queueAddBadgeText;

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
    // elapsed time. Seeded from the persisted choice; the user flips it by
    // clicking the label, and the choice persists across launches.
    private bool _showRemaining;

    // The last rendered position, so a label toggle re-renders immediately from
    // it instead of waiting for the next progress tick. Cleared when the bar
    // hides, so a stale position can't resurrect via a later toggle.
    private PlaybackPositionRender? _lastPosition;

    public NowPlayingBarController(
        SessionStore session,
        PlaybackStore playback,
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
        Button mute,
        Button repeat,
        Button previous,
        Button next,
        ProgressRing loading,
        Border queueAddBadge,
        ScaleTransform queueAddBadgeScale,
        TextBlock queueAddBadgeText)
    {
        _session = session;
        _playback = playback;
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
        _mute = mute;
        _repeat = repeat;
        _previous = previous;
        _next = next;
        _loading = loading;
        _queueAddBadge = queueAddBadge;
        _queueAddBadgeScale = queueAddBadgeScale;
        _queueAddBadgeText = queueAddBadgeText;

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
        _showRemaining = TimeLabelStore.Load();
        _elapsed.Tapped += (_, _) => ToggleTimeLabel();
        ToolTipService.SetToolTip(_elapsed, Loc.Chrome(PlaybackPositionModel.TimeLabelTooltipKey(_showRemaining)));

        _playback.NowPlayingChanged += OnNowPlayingChanged;
        _playback.PlaybackStopped += OnPlaybackStopped;
        _playback.LoadingStarted += OnLoadingStarted;
        _playback.PositionChanged += OnPositionChanged;
        _playback.VolumeChanged += OnVolumeChanged;
        _playback.MuteChanged += OnMuteChanged;
        _playback.RepeatChanged += OnRepeatChanged;
        _playback.TransportChanged += OnTransportChanged;
        _playback.QueueItemsAdded += OnQueueItemsAdded;
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
    }

    private void OnNowPlayingChanged(NowPlayingBarTrack track)
    {
        _nowPlayingBar.Visibility = Visibility.Visible;
        _title.Text = track.Title;
        _artist.Text = track.Artist;
        _playPause.Content = track.IsPlaying ? "⏸" : "▶";
        _cover.Source = CoverImage.LoadImage(_session.CurrentHandleOrNull(), track.CoverImageId);
        // Audio is flowing: drop the buffering spinner, restore the play/pause
        // control.
        _loading.IsActive = false;
        _loading.Visibility = Visibility.Collapsed;
        _playPause.Visibility = Visibility.Visible;
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
        _playPause.Visibility = Visibility.Visible;
    }

    private void OnLoadingStarted()
    {
        // Core is preparing or buffering the track (initial load, or a seek to a
        // position not yet downloaded). Show the bar with a spinner over the
        // transport; the prior track's title/cover stay until PlaybackPlaying lands.
        _nowPlayingBar.Visibility = Visibility.Visible;
        _playPause.Visibility = Visibility.Collapsed;
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

    // Write both time labels: the leading label shows elapsed or a minus-prefixed
    // remaining countdown per the current mode; the trailing label is always the
    // track total. Remembers the position so a label toggle re-renders from it.
    private void RenderTimeLabels(PlaybackPositionRender render)
    {
        _lastPosition = render;
        _elapsed.Text = PlaybackPositionModel.PositionLabel(_showRemaining, render.PositionMs, render.DurationMs);
        _duration.Text = PlaybackPositionModel.DurationLabel(render.DurationMs);
    }

    // Flip the leading label between elapsed and remaining, persist the choice,
    // update the tooltip to name the new target mode, and re-render immediately
    // from the last position so the label changes without waiting for the next
    // progress tick. With no position yet there is nothing to re-render — the
    // next tick picks the mode up.
    private void ToggleTimeLabel()
    {
        _showRemaining = !_showRemaining;
        TimeLabelStore.Save(_showRemaining);
        ToolTipService.SetToolTip(_elapsed, Loc.Chrome(PlaybackPositionModel.TimeLabelTooltipKey(_showRemaining)));
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
    }

    private void OnMuteChanged(bool isMuted)
    {
        _mute.Content = isMuted ? "🔇" : "🔊";
    }

    private void OnRepeatChanged(BridgeRepeatMode mode)
    {
        _repeat.Content = mode switch
        {
            BridgeRepeatMode.Track => "🔂",
            BridgeRepeatMode.Context => "🔁",
            _ => "↻",
        };
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
