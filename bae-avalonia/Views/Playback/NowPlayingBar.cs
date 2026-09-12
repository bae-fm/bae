using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The docked playback strip along the bottom of the shell: the playing track on
/// the left, the transport and seek bar in the center, the cast / queue / volume
/// cluster on the right. The macOS <c>NowPlayingBar</c>'s counterpart.
///
/// Every control reads <see cref="PlaybackStore"/> — core's playback state — and
/// writes through <see cref="PlaybackCommands"/>, which names the absolute state
/// it wants. Nothing here keeps a second copy of what is playing: the bar holds
/// only the last values it was handed, so it can re-render itself without
/// waiting for the next event.
///
/// The cast control and the queue toggle are handed in: they belong to the queue
/// pane and the cast picker, not to the transport.
/// </summary>
internal sealed class NowPlayingBar : UserControl
{
    private const double CoverSize = 54;

    private readonly PlaybackStore _store;
    private readonly SettingsStore _settings;
    private readonly PlaybackCommands _commands;
    private readonly ImageStore _images;
    private readonly Action<string> _navigateToAlbum;

    private readonly Image _cover = new() { Stretch = Stretch.UniformToFill };
    private readonly Button _coverButton;
    private readonly TextBlock _title;
    private readonly Button _titleButton;
    private readonly TextBlock _secondary;
    private readonly Button _continue;

    private readonly Button _shuffle;
    private readonly PathIcon _shuffleGlyph;
    private readonly Button _playPause;
    private readonly PathIcon _playPauseGlyph;
    private readonly Button _repeat;
    private readonly PathIcon _repeatGlyph;

    private readonly Button _leadingClockButton;
    private readonly TextBlock _leadingClock;
    private readonly TextBlock _trailingClock;
    private readonly Slider _progress;

    private readonly Button _mute;
    private readonly PathIcon _muteGlyph;
    private readonly Slider _volume;

    private NowPlayingBarTrack? _track;
    private PlaybackPositionRender? _position;

    // Set while the bar is writing a slider from playback state, so the change
    // that write raises is not read back as a command.
    private bool _applying;
    // Set between the press and the release on each slider: the thumb belongs to
    // the pointer then, and an arriving playback value must not move it.
    private bool _scrubbing;
    private bool _adjustingVolume;

    public NowPlayingBar(
        PlaybackStore store,
        SettingsStore settings,
        PlaybackCommands commands,
        ImageStore images,
        Action<string> navigateToAlbum,
        Control castControl,
        Control queueButton)
    {
        _store = store;
        _settings = settings;
        _commands = commands;
        _images = images;
        _navigateToAlbum = navigateToAlbum;

        var grid = new Grid
        {
            Margin = new Thickness(20, 12),
            VerticalAlignment = VerticalAlignment.Center,
            ColumnDefinitions = new ColumnDefinitions("*,Auto,*"),
        };

        // ── Left: cover, title, and the secondary line ───────────────────────
        var coverFrame = new Border
        {
            Width = CoverSize,
            Height = CoverSize,
            CornerRadius = new CornerRadius(10),
            ClipToBounds = true,
            Child = _cover,
        };
        SetBg(coverFrame, "BaeElevatedBrush");
        _coverButton = Bare(coverFrame);
        Describe(_coverButton, Loc.Chrome("nowplaying.go_to_album"));
        _coverButton.Click += (_, _) => NavigateToAlbum();

        _title = new TextBlock
        {
            FontSize = 15,
            FontWeight = FontWeight.Bold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        _title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        _titleButton = Bare(_title);
        _titleButton.HorizontalAlignment = HorizontalAlignment.Left;
        Describe(_titleButton, Loc.Chrome("nowplaying.go_to_album"));
        _titleButton.Click += (_, _) => NavigateToAlbum();

        _secondary = new TextBlock
        {
            FontSize = 13,
            FontWeight = FontWeight.Medium,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        _secondary[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        // Resuming past a side break is an ordinary resume; the button is here
        // because the break is where someone has to turn the record over.
        _continue = new Button
        {
            Content = Loc.Chrome("action.play"),
            FontSize = 11,
            Padding = new Thickness(10, 1),
            IsVisible = false,
            VerticalAlignment = VerticalAlignment.Center,
        };
        _continue.Classes.Add("accent");
        _continue.Click += (_, _) => _commands.PlayPause();

        var secondaryRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        secondaryRow.Children.Add(_secondary);
        secondaryRow.Children.Add(_continue);

        var text = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(_titleButton);
        text.Children.Add(secondaryRow);

        var left = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
            VerticalAlignment = VerticalAlignment.Center,
        };
        left.Children.Add(_coverButton);
        left.Children.Add(text);
        Grid.SetColumn(left, 0);
        grid.Children.Add(left);

        // ── Center: transport over the seek bar ──────────────────────────────
        (_shuffle, _shuffleGlyph) = ToggleButton(Icons.Shuffle, 16, 30);
        _shuffle.Click += (_, _) => _commands.SetShuffle(_store.Context?.Shuffled != true);
        (_repeat, _repeatGlyph) = ToggleButton(Icons.Repeat, 16, 30);
        _repeat.Click += (_, _) => _commands.CycleRepeatMode();

        var previous = Icons.IconButton(Icons.SkipPrevious, 20, "BaeTextPrimaryBrush", 34);
        Describe(previous, Loc.Chrome("nowplaying.previous"));
        previous.Click += (_, _) => _commands.PreviousTrack();
        var next = Icons.IconButton(Icons.SkipNext, 20, "BaeTextPrimaryBrush", 34);
        Describe(next, Loc.Chrome("nowplaying.next"));
        next.Click += (_, _) => _commands.NextTrack();

        _playPauseGlyph = Icons.Glyph(Icons.Play, 20, "BaeTextPrimaryBrush");
        _playPause = new Button
        {
            Width = 48,
            Height = 48,
            CornerRadius = new CornerRadius(24),
            Padding = new Thickness(0),
            BorderThickness = new Thickness(0),
            HorizontalContentAlignment = HorizontalAlignment.Center,
            VerticalContentAlignment = VerticalAlignment.Center,
            Content = _playPauseGlyph,
        };
        _playPause[!BackgroundProperty] = new DynamicResourceExtension("BaeTileBrush");
        _playPause.Click += (_, _) => _commands.PlayPause();

        var transport = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 22,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        transport.Children.Add(_shuffle);
        transport.Children.Add(previous);
        transport.Children.Add(_playPause);
        transport.Children.Add(next);
        transport.Children.Add(_repeat);

        _leadingClock = ClockLabel();
        _leadingClock.MinWidth = 44;
        _leadingClock.TextAlignment = TextAlignment.Right;
        _leadingClockButton = Bare(_leadingClock);
        _leadingClockButton.Click += (_, _) => _commands.ToggleShowRemainingTime();
        _trailingClock = ClockLabel();

        _progress = new Slider
        {
            Minimum = 0,
            Maximum = 1,
            VerticalAlignment = VerticalAlignment.Center,
        };
        Describe(_progress, Loc.Chrome("nowplaying.position"));
        _progress.ValueChanged += OnProgressChanged;
        Track(_progress, dragging => _scrubbing = dragging, CommitSeek, _store.ClearSeekProjection);

        var scrubber = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto"), ColumnSpacing = 11 };
        Grid.SetColumn(_leadingClockButton, 0);
        Grid.SetColumn(_progress, 1);
        Grid.SetColumn(_trailingClock, 2);
        scrubber.Children.Add(_leadingClockButton);
        scrubber.Children.Add(_progress);
        scrubber.Children.Add(_trailingClock);

        var center = new StackPanel { Width = 460, VerticalAlignment = VerticalAlignment.Center, Spacing = 6 };
        center.Children.Add(transport);
        center.Children.Add(scrubber);
        Grid.SetColumn(center, 1);
        grid.Children.Add(center);

        // ── Right: cast, queue, mute, volume ─────────────────────────────────
        _muteGlyph = Icons.Glyph(Icons.VolumeUp, 16, "BaeTextSecondaryBrush");
        _mute = new Button
        {
            Width = 30,
            Height = 30,
            Padding = new Thickness(0),
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            HorizontalContentAlignment = HorizontalAlignment.Center,
            VerticalContentAlignment = VerticalAlignment.Center,
            Content = _muteGlyph,
        };
        _mute.Click += (_, _) => _commands.ToggleMute();

        _volume = new Slider
        {
            Minimum = 0,
            Maximum = 1,
            Width = 96,
            VerticalAlignment = VerticalAlignment.Center,
        };
        Describe(_volume, Loc.Chrome("nowplaying.volume"));
        _volume.ValueChanged += OnVolumeChanged;
        Track(_volume, dragging => _adjustingVolume = dragging, CommitVolume, () => { });

        var right = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Center,
        };
        right.Children.Add(castControl);
        right.Children.Add(queueButton);
        right.Children.Add(_mute);
        right.Children.Add(_volume);
        Grid.SetColumn(right, 2);
        grid.Children.Add(right);

        var bar = new Border { BorderThickness = new Thickness(0, 1, 0, 0), Child = grid };
        SetBg(bar, "BaeSurfaceBrush");
        SetBorder(bar, "BaeHairlineBrush");
        Content = bar;

        RenderNowPlaying();
        RenderTransport();
        RenderPosition();
        RenderVolume();
    }

    // The bar's whole content follows the store, so it listens only while it is
    // on screen and holds no subscription once it leaves the tree.
    protected override void OnAttachedToVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        _store.NowPlayingChanged += OnNowPlayingChanged;
        _store.PlaybackStopped += OnPlaybackStopped;
        _store.LoadingStarted += OnLoadingStarted;
        _store.PositionChanged += OnPositionChanged;
        _store.VolumeChanged += OnVolumeReported;
        _store.MuteChanged += OnMuteReported;
        _store.RepeatChanged += OnRepeatReported;
        _store.QueueChanged += RenderTransport;
        _settings.Changed += RenderPosition;
        // The store holds the transport, mute, volume, repeat, and shuffle state
        // as snapshots, so a bar returning to the tree reads them rather than
        // waiting for the next change; the track and its position are the last
        // ones it was handed.
        RenderNowPlaying();
        RenderTransport();
        RenderPosition();
        RenderVolume();
    }

    protected override void OnDetachedFromVisualTree(VisualTreeAttachmentEventArgs e)
    {
        _store.NowPlayingChanged -= OnNowPlayingChanged;
        _store.PlaybackStopped -= OnPlaybackStopped;
        _store.LoadingStarted -= OnLoadingStarted;
        _store.PositionChanged -= OnPositionChanged;
        _store.VolumeChanged -= OnVolumeReported;
        _store.MuteChanged -= OnMuteReported;
        _store.RepeatChanged -= OnRepeatReported;
        _store.QueueChanged -= RenderTransport;
        _settings.Changed -= RenderPosition;
        base.OnDetachedFromVisualTree(e);
    }

    // ── Store events ─────────────────────────────────────────────────────────
    private void OnNowPlayingChanged(NowPlayingBarTrack track)
    {
        _track = track;
        RenderNowPlaying();
        RenderTransport();
        RenderPosition();
    }

    private void OnPlaybackStopped()
    {
        _track = null;
        _position = null;
        RenderNowPlaying();
        RenderTransport();
        RenderPosition();
    }

    private void OnLoadingStarted()
    {
        RenderTransport();
        RenderPosition();
    }

    private void OnPositionChanged(PlaybackPositionRender position)
    {
        _position = position;
        RenderPosition();
    }

    private void OnVolumeReported(float volume) => RenderVolume();

    private void OnMuteReported(bool muted) => RenderVolume();

    private void OnRepeatReported(BridgeRepeatMode mode) => RenderTransport();

    // ── Rendering ────────────────────────────────────────────────────────────
    private void RenderNowPlaying()
    {
        var track = _track;
        _coverButton.IsEnabled = track is not null && _store.NowPlayingAlbumId is not null;
        _titleButton.IsEnabled = _coverButton.IsEnabled;

        if (track is null)
        {
            _title.Text = string.Empty;
            _secondary.Text = string.Empty;
            _continue.IsVisible = false;
            _images.Bind(_cover, null, ImageWidths.Row);
            return;
        }

        _title.Text = track.Title;
        // While playback waits at a side boundary, the line core composed for the
        // break takes the artist names' place — that is what the person needs to
        // read there.
        if (track.SidePausePrompt is { } prompt)
        {
            _secondary.Text = Loc.Core(prompt.TitleKey, "label", prompt.SideLabel);
            _continue.IsVisible = true;
        }
        else
        {
            _secondary.Text = track.Artist;
            _continue.IsVisible = false;
        }
        _images.Bind(_cover, ImageContent.ForLibraryImage(track.CoverImage), ImageWidths.Row);
    }

    private void RenderTransport()
    {
        var playing = _store.PlayState == TransportPlayState.Playing;
        _playPauseGlyph.Data = Geometry.Parse(playing ? Icons.Pause : Icons.Play);
        Describe(_playPause, Loc.Chrome(playing ? "nowplaying.pause" : "action.play"));

        // No playing context means no order to shuffle, so the control says so
        // rather than doing nothing when pressed.
        var shuffled = _store.Context?.Shuffled;
        _shuffle.IsEnabled = shuffled is not null;
        _shuffle.Opacity = shuffled is null ? 0.4 : 1;
        StyleToggle(_shuffle, _shuffleGlyph, shuffled == true);
        Describe(_shuffle, Loc.Chrome(shuffled == true ? "queue.shuffle.off" : "queue.shuffle.on"));

        var mode = _store.RepeatMode;
        _repeatGlyph.Data = Geometry.Parse(mode == BridgeRepeatMode.Track ? Icons.RepeatOne : Icons.Repeat);
        StyleToggle(_repeat, _repeatGlyph, mode != BridgeRepeatMode.Off);
        Describe(_repeat, Loc.Chrome(mode switch
        {
            BridgeRepeatMode.Context => "nowplaying.repeat_context",
            BridgeRepeatMode.Track => "nowplaying.repeat_track",
            _ => "nowplaying.repeat_off",
        }));
    }

    private void RenderPosition()
    {
        var showRemaining = _commands.ShowRemainingTime;
        Describe(_leadingClockButton, Loc.Chrome(PlaybackPositionModel.TimeLabelTooltipKey(showRemaining)));

        // A track core is still preparing has no position to drop a seek on, and
        // a stopped bar has no track at all.
        _progress.IsEnabled = _track is not null && !_store.IsLoading;
        if (_position is not { } position)
        {
            _leadingClock.Text = string.Empty;
            _trailingClock.Text = string.Empty;
            Apply(_progress, 0);
            return;
        }

        var (leading, trailing) = BridgeDisplay.SeekBarClocks(
            position.PositionMs, position.DurationMs, showRemaining);
        _leadingClock.Text = leading;
        _trailingClock.Text = trailing;
        if (!_scrubbing)
        {
            Apply(_progress, Math.Clamp(position.Progress, _progress.Minimum, _progress.Maximum));
        }
    }

    private void RenderVolume()
    {
        var muted = _store.IsMuted;
        var volume = _store.Volume;
        // The rendered level picks the speaker glyph: silenced when muted or at
        // zero, one wave up to the midpoint, two above it.
        _muteGlyph.Data = Geometry.Parse(
            muted || volume == 0 ? Icons.VolumeOff : volume <= 0.55f ? Icons.VolumeDown : Icons.VolumeUp);
        Describe(_mute, Loc.Chrome(muted ? "nowplaying.unmute" : "nowplaying.mute"));
        if (!_adjustingVolume)
        {
            Apply(_volume, Math.Clamp(volume, _volume.Minimum, _volume.Maximum));
        }
    }

    // ── Slider input ─────────────────────────────────────────────────────────
    private void OnProgressChanged(object? sender, RangeBaseValueChangedEventArgs e)
    {
        if (_applying)
        {
            return;
        }
        // The dropped position is shown until core confirms the seek, so the
        // labels stop following playback the moment the thumb moves.
        if (_store.ProjectSeek(e.NewValue, _progress.Minimum, _progress.Maximum) is { } projection)
        {
            var (leading, trailing) = BridgeDisplay.SeekBarClocks(
                checked((long)projection.TargetPositionMs), projection.DurationMs, _commands.ShowRemainingTime);
            _leadingClock.Text = leading;
            _trailingClock.Text = trailing;
        }
        // A keyboard step has no press to release, so it commits where it lands.
        if (!_scrubbing)
        {
            CommitSeek();
        }
    }

    private void CommitSeek() => _commands.SeekByRatio(_progress.Value);

    private void OnVolumeChanged(object? sender, RangeBaseValueChangedEventArgs e)
    {
        if (!_applying)
        {
            // Volume is what the output is doing right now, so it follows the
            // thumb rather than waiting for the release.
            CommitVolume();
        }
    }

    private void CommitVolume() => _commands.SetVolume((float)_volume.Value);

    private void Apply(Slider slider, double value)
    {
        _applying = true;
        slider.Value = value;
        _applying = false;
    }

    // Follow one slider's drag: the press takes the thumb, the release commits
    // where it was dropped, and losing the pointer without a release abandons it.
    private static void Track(Slider slider, Action<bool> setDragging, Action commit, Action cancel)
    {
        slider.AddHandler(
            PointerPressedEvent,
            (_, _) => setDragging(true),
            RoutingStrategies.Tunnel,
            handledEventsToo: true);
        slider.AddHandler(
            PointerReleasedEvent,
            (_, _) =>
            {
                setDragging(false);
                commit();
            },
            RoutingStrategies.Tunnel | RoutingStrategies.Bubble,
            handledEventsToo: true);
        slider.AddHandler(
            PointerCaptureLostEvent,
            (_, _) =>
            {
                setDragging(false);
                cancel();
            },
            RoutingStrategies.Tunnel | RoutingStrategies.Bubble,
            handledEventsToo: true);
    }

    private void NavigateToAlbum()
    {
        if (_store.NowPlayingAlbumId is { } albumId)
        {
            _navigateToAlbum(albumId);
        }
    }

    // ── Chrome ───────────────────────────────────────────────────────────────
    // Shuffle and repeat share one 30px slot: an accent glyph over a soft accent
    // fill while the mode is on, neutral otherwise.
    private static (Button Button, PathIcon Glyph) ToggleButton(string data, double glyphSize, double box)
    {
        var glyph = Icons.Glyph(data, glyphSize, "BaeTextSecondaryBrush");
        var button = new Button
        {
            Width = box,
            Height = box,
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(0),
            BorderThickness = new Thickness(0),
            HorizontalContentAlignment = HorizontalAlignment.Center,
            VerticalContentAlignment = VerticalAlignment.Center,
            Content = glyph,
        };
        return (button, glyph);
    }

    private static void StyleToggle(Button button, PathIcon glyph, bool active)
    {
        glyph[!PathIcon.ForegroundProperty] = new DynamicResourceExtension(
            active ? "BaeAccentBrush" : "BaeTextSecondaryBrush");
        if (active)
        {
            button[!BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
        }
        else
        {
            button.Background = Brushes.Transparent;
        }
    }

    // A button that is only a hit target around its content: the cover, the
    // title, and the clock label each read as text and act as a control.
    private static Button Bare(Control content) => new()
    {
        Padding = new Thickness(0),
        Background = Brushes.Transparent,
        BorderThickness = new Thickness(0),
        CornerRadius = new CornerRadius(0),
        Content = content,
    };

    private static TextBlock ClockLabel()
    {
        var label = new TextBlock
        {
            MinWidth = 34,
            VerticalAlignment = VerticalAlignment.Center,
            FontSize = 11.5,
            FontWeight = FontWeight.SemiBold,
        };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return label;
    }

    private static void Describe(Control control, string name)
    {
        Avalonia.Automation.AutomationProperties.SetName(control, name);
        ToolTip.SetTip(control, name);
    }

    private static void SetBg(Border border, string key) =>
        border[!Border.BackgroundProperty] = new DynamicResourceExtension(key);

    private static void SetBorder(Border border, string key) =>
        border[!Border.BorderBrushProperty] = new DynamicResourceExtension(key);
}
