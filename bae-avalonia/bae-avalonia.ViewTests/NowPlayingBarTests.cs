using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The now-playing bar: what it shows for the playback state the store holds,
/// and what each of its controls sends when pressed. The bar keeps no playback
/// state of its own, so every case here drives the store and reads the bar.
/// </summary>
public sealed class NowPlayingBarTests
{
    [AvaloniaFact]
    public void ThePlayingTrackFillsTheTitleAndTheSecondaryLine()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPlaying("album", "track", "Sister Ray", "The Velvet Underground", null);

        Assert.Equal("Sister Ray", Title(bar));
        Assert.Equal("The Velvet Underground", Secondary(bar));
    }

    [AvaloniaFact]
    public void StoppingEmptiesTheTrackLines()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPlaying("album", "track", "Sister Ray", "The Velvet Underground", null);
        fakes.Store.ApplyStopped();

        Assert.Equal(string.Empty, Title(bar));
        Assert.Equal(string.Empty, Secondary(bar));
    }

    /// <summary>The glyph follows the transport, and a press names the opposite
    /// of what is current — so playing pauses and paused resumes.</summary>
    [AvaloniaFact]
    public void ThePlayButtonShowsAndSendsWhatTheTransportIs()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPlaying("album", "track", "Title", "Artist", null);

        Assert.Equal(Loc.Chrome("nowplaying.pause"), Name(PlayPause(bar)));
        Press(PlayPause(bar));
        Assert.Equal(new[] { "pause" }, fakes.Ran);

        fakes.Store.ApplyPaused("album", "track", "Title", "Artist", null, new BridgePlaybackPauseReason.Manual());

        Assert.Equal(Loc.Chrome("action.play"), Name(PlayPause(bar)));
        Press(PlayPause(bar));
        Assert.Equal(new[] { "pause", "resume" }, fakes.Ran);
    }

    [AvaloniaFact]
    public void EachTransportButtonRunsItsCommandOnce()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyQueueValue(Snapshot(shuffled: false));

        Press(Previous(bar));
        Press(Next(bar));
        Press(Shuffle(bar));
        Press(Repeat(bar));
        Press(Mute(bar));

        Assert.Equal(
            new[] { "previous track", "next track", "set shuffle True", "set repeat Context", "mute" },
            fakes.Ran);
    }

    /// <summary>Shuffle names the order it wants, so pressing it while shuffled
    /// asks for sequential.</summary>
    [AvaloniaFact]
    public void ShuffleAsksForTheOppositeOrderAndTintsWhileOn()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyQueueValue(Snapshot(shuffled: true));

        Assert.Equal(Loc.Chrome("queue.shuffle.off"), Name(Shuffle(bar)));
        Press(Shuffle(bar));

        Assert.Equal(new[] { "set shuffle False" }, fakes.Ran);
    }

    /// <summary>With nothing playing from a context there is no order to set, so
    /// the control is unavailable rather than doing nothing when pressed.</summary>
    [AvaloniaFact]
    public void ShuffleIsUnavailableWithNoPlayingContext()
    {
        var (bar, fakes) = Build();

        Assert.False(Shuffle(bar).IsEffectivelyEnabled);

        fakes.Store.ApplyQueueValue(Snapshot(shuffled: false));

        Assert.True(Shuffle(bar).IsEffectivelyEnabled);
    }

    [AvaloniaFact]
    public void RepeatNamesTheModeTheStoreHolds()
    {
        var (bar, fakes) = Build();

        Assert.Equal(Loc.Chrome("nowplaying.repeat_off"), Name(Repeat(bar)));

        fakes.Store.ApplyRepeat(BridgeRepeatMode.Context);
        Assert.Equal(Loc.Chrome("nowplaying.repeat_context"), Name(Repeat(bar)));

        fakes.Store.ApplyRepeat(BridgeRepeatMode.Track);
        Assert.Equal(Loc.Chrome("nowplaying.repeat_track"), Name(Repeat(bar)));
    }

    [AvaloniaFact]
    public void MuteNamesTheActionAPressPerforms()
    {
        var (bar, fakes) = Build();

        Assert.Equal(Loc.Chrome("nowplaying.mute"), Name(Mute(bar)));

        fakes.Store.ApplyMute(true);

        Assert.Equal(Loc.Chrome("nowplaying.unmute"), Name(Mute(bar)));
    }

    [AvaloniaFact]
    public void TheVolumeSliderFollowsWhatCoreReports()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyVolume(0.25f);

        Assert.Equal(0.25, Volume(bar).Value, 3);
    }

    /// <summary>Moving the slider sends the level it stands at, and the value
    /// core reports back does not send it again.</summary>
    [AvaloniaFact]
    public void MovingTheVolumeSliderSetsThatLevelOnce()
    {
        var (bar, fakes) = Build();

        Volume(bar).Value = 0.4;

        Assert.Equal(new[] { "set volume 0.4" }, fakes.Ran);

        fakes.Store.ApplyVolume(0.4f);

        Assert.Equal(new[] { "set volume 0.4" }, fakes.Ran);
    }

    /// <summary>The leading label offers the clock a click switches to, which is
    /// the opposite of the one it is showing.</summary>
    [AvaloniaFact]
    public void TheLeadingClockOffersTheOtherClockAndWritesThePreference()
    {
        var (bar, fakes) = Build();

        Assert.Equal(Loc.Chrome("nowplaying.show_remaining"), Name(LeadingClock(bar)));
        Press(LeadingClock(bar));
        Assert.Equal(new[] { "show remaining True" }, fakes.Ran);

        fakes.ShowRemainingTime = true;
        fakes.Settings.Reload();

        Assert.Equal(Loc.Chrome("nowplaying.show_elapsed"), Name(LeadingClock(bar)));
        Press(LeadingClock(bar));
        Assert.Equal(new[] { "show remaining True", "show remaining False" }, fakes.Ran);
    }

    /// <summary>Nothing playing, and a track core is still preparing, both leave
    /// the seek bar with no position to drop a seek on.</summary>
    [AvaloniaFact]
    public void TheSeekBarIsUnavailableWithNoTrackAndWhileOneLoads()
    {
        var (bar, fakes) = Build();

        Assert.False(Progress(bar).IsEffectivelyEnabled);

        fakes.Store.ApplyPlaying("album", "track", "Title", "Artist", null);
        Assert.True(Progress(bar).IsEffectivelyEnabled);

        fakes.Store.ApplyLoading("next", null);
        Assert.False(Progress(bar).IsEffectivelyEnabled);

        fakes.Store.ApplyPlaying("album", "next", "Next", "Artist", null);
        Assert.True(Progress(bar).IsEffectivelyEnabled);
    }

    /// <summary>The side break replaces the artist names with the line core
    /// composed for it, and offers the resume that carries playback past it.</summary>
    [AvaloniaFact]
    public void TheSideBreakLineAndItsContinueAppearWhilePlaybackWaits()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPaused(
            "album", "track", "Title", "Artist", null, SideEnded("A"));

        Assert.Equal(
            Loc.Core("core.playback.pause.side_ended.title", "label", "A"),
            Secondary(bar));
        var carryOn = Continue(bar);
        Assert.True(carryOn.IsVisible);

        Press(carryOn);
        Assert.Equal(new[] { "resume" }, fakes.Ran);

        fakes.Store.ApplyPlaying("album", "track", "Title", "Artist", null);

        Assert.Equal("Artist", Secondary(bar));
        Assert.False(Continue(bar).IsVisible);
    }

    /// <summary>A position update moves the thumb and writes both clocks from
    /// the one projection core computed.</summary>
    [AvaloniaFact]
    public void APositionUpdateMovesTheThumbAndWritesBothClocks()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPlaying("album", "track", "Title", "Artist", null);
        fakes.Store.ApplyProgress("track", 55_500, 222_000, 0.25);

        Assert.Equal(0.25, Progress(bar).Value, 3);
        var (leading, trailing) = BridgeDisplay.SeekBarClocks(55_500, 222_000, showRemaining: false);
        Assert.Equal(leading, ClockText(bar, leadingLabel: true));
        Assert.Equal(trailing, ClockText(bar, leadingLabel: false));
    }

    /// <summary>Turning the preference on counts the same position down instead
    /// of up; both labels come from one projection, so the trailing total is
    /// unaffected.</summary>
    [AvaloniaFact]
    public void TurningOnRemainingTimeRewritesTheLeadingClock()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPlaying("album", "track", "Title", "Artist", null);
        fakes.Store.ApplyProgress("track", 55_500, 222_000, 0.25);
        var elapsed = ClockText(bar, leadingLabel: true);

        fakes.ShowRemainingTime = true;
        fakes.Settings.Reload();

        var remaining = BridgeDisplay.SeekBarClocks(55_500, 222_000, showRemaining: true).Leading;
        Assert.Equal(remaining, ClockText(bar, leadingLabel: true));
        Assert.NotEqual(elapsed, ClockText(bar, leadingLabel: true));
    }

    /// <summary>Dropping the thumb projects the position it was dropped on and
    /// seeks there; the projection is what the labels read until core confirms
    /// it.</summary>
    [AvaloniaFact]
    public void DroppingTheThumbSeeksToWhereItLanded()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPlaying("album", "track", "Title", "Artist", null);
        fakes.Store.ApplyProgress("track", 0, 200_000, 0);

        Progress(bar).Value = 0.5;

        Assert.Equal(new[] { "seek 0.5" }, fakes.Ran);
        Assert.Equal(
            BridgeDisplay.SeekBarClocks(100_000, 200_000, showRemaining: false).Leading,
            ClockText(bar, leadingLabel: true));
    }

    /// <summary>A subscription lives only while the bar is on screen, and the bar
    /// reads the store again when it comes back rather than waiting for the next
    /// change.</summary>
    [AvaloniaFact]
    public void ADetachedBarStopsListeningAndRereadsOnItsReturn()
    {
        var (bar, fakes) = Build();
        var host = (Panel)bar.Parent!;

        host.Children.Remove(bar);
        Dispatcher.UIThread.RunJobs();
        fakes.Store.ApplyMute(true);

        Assert.Equal(Loc.Chrome("nowplaying.mute"), Name(Mute(bar)));

        host.Children.Add(bar);
        Dispatcher.UIThread.RunJobs();

        Assert.Equal(Loc.Chrome("nowplaying.unmute"), Name(Mute(bar)));
    }

    // ── Fixture ──────────────────────────────────────────────────────────────

    /// <summary>Every command the bar can send, recorded in order, over services
    /// that report what was asked for instead of reaching a library.</summary>
    private sealed class Fakes
    {
        public readonly List<string> Ran = new();
        public PlaybackStore Store = null!;
        public SettingsStore Settings = null!;
        public bool ShowRemainingTime;
    }

    private static (NowPlayingBar Bar, Fakes Fakes) Build()
    {
        Dispatcher.UIThread.VerifyAccess();

        var fakes = new Fakes();
        fakes.Store = new PlaybackStore(new QueueService(), _ => { });
        fakes.Settings = new SettingsStore(new SettingsService
        {
            GetSettings = () => (true, new Settings { ShowRemainingTime = fakes.ShowRemainingTime }),
        });
        fakes.Settings.Reload();

        bool Ran(string command)
        {
            fakes.Ran.Add(command);
            return true;
        }

        var playback = new PlaybackService
        {
            Pause = () => Ran("pause"),
            Resume = () => Ran("resume"),
            NextTrack = () => Ran("next track"),
            PreviousTrack = () => Ran("previous track"),
            SeekByRatio = ratio => Ran($"seek {ratio}"),
            SetVolume = volume => Ran($"set volume {volume}"),
            SetMuted = _ => Ran("mute"),
            // Which mode follows which is core's; the fake answers with one the
            // bar never sets directly, so a cycle is told apart from a pick.
            NextRepeatMode = _ => BridgeRepeatMode.Context,
            SetRepeatMode = mode => Ran($"set repeat {mode}"),
            SetShowRemainingTime = enabled =>
            {
                fakes.Ran.Add($"show remaining {enabled}");
                return (true, null);
            },
        };
        var queue = new QueueService { SetShuffle = on => Ran($"set shuffle {on}") };

        var commands = new PlaybackCommands(
            playback,
            queue,
            fakes.Store,
            fakes.Settings,
            () => 1,
            () => false,
            _ => { },
            _ => { });

        var bar = new NowPlayingBar(
            fakes.Store,
            fakes.Settings,
            commands,
            new ImageStore(),
            _ => { },
            new Panel(),
            new Button());
        // The bar listens only while it is on screen, so every case runs it in a
        // shown window rather than bare.
        var host = new Panel();
        host.Children.Add(bar);
        new Window { Content = host }.Show();
        Dispatcher.UIThread.RunJobs();
        return (bar, fakes);
    }

    private static BridgeQueueSnapshot Snapshot(bool shuffled) => new(
        Manual: Array.Empty<BridgeQueueEntry>(),
        Context: new BridgePlaybackContext(
            Kind: BridgePlaybackSourceKind.Release,
            SourceTitle: "Album",
            Shuffled: shuffled,
            Upcoming: Array.Empty<BridgeQueueEntry>(),
            UpcomingTotal: 0),
        HasPrevious: true,
        HasNext: true,
        Revision: 1);

    private static BridgePlaybackPauseReason SideEnded(string label) =>
        new BridgePlaybackPauseReason.SideEnded(new BridgeSidePausePrompt(
            Id: $"side-{label}",
            TitleKey: "core.playback.pause.side_ended.title",
            SideLabel: label,
            MessageKey: "core.playback.pause.side_ended.message.vinyl"));

    // The bar's two track lines, told apart by the type they are set in.
    private static string? Title(NowPlayingBar bar) => Line(bar, size: 15);

    private static string? Secondary(NowPlayingBar bar) => Line(bar, size: 13);

    private static string? Line(NowPlayingBar bar, double size) =>
        bar.GetVisualDescendants().OfType<TextBlock>().Single(block => block.FontSize == size).Text;

    private static string? ClockText(NowPlayingBar bar, bool leadingLabel) =>
        Clocks(bar).ElementAt(leadingLabel ? 0 : 1).Text;

    private static IEnumerable<TextBlock> Clocks(NowPlayingBar bar) =>
        bar.GetVisualDescendants().OfType<TextBlock>().Where(block => block.FontSize == 11.5);

    // Each control is found by the action it offers, which is what it announces
    // to a screen reader and shows in its tooltip.
    private static Button PlayPause(NowPlayingBar bar) =>
        Named(bar, "action.play", "nowplaying.pause");

    private static Button Previous(NowPlayingBar bar) => Named(bar, "nowplaying.previous");

    private static Button Next(NowPlayingBar bar) => Named(bar, "nowplaying.next");

    private static Button Shuffle(NowPlayingBar bar) =>
        Named(bar, "queue.shuffle.on", "queue.shuffle.off");

    private static Button Repeat(NowPlayingBar bar) =>
        Named(bar, "nowplaying.repeat_off", "nowplaying.repeat_context", "nowplaying.repeat_track");

    private static Button Mute(NowPlayingBar bar) =>
        Named(bar, "nowplaying.mute", "nowplaying.unmute");

    private static Button LeadingClock(NowPlayingBar bar) =>
        Named(bar, "nowplaying.show_remaining", "nowplaying.show_elapsed");

    private static Button Continue(NowPlayingBar bar) =>
        Buttons(bar).Single(button => Equals(button.Content, Loc.Chrome("action.play")));

    private static Button Named(NowPlayingBar bar, params string[] keys)
    {
        var names = keys.Select(Loc.Chrome).ToHashSet();
        return Buttons(bar).Single(button => Name(button) is { } name && names.Contains(name));
    }

    private static Slider Progress(NowPlayingBar bar) =>
        bar.GetVisualDescendants().OfType<Slider>().Single(slider => double.IsNaN(slider.Width));

    private static Slider Volume(NowPlayingBar bar) =>
        bar.GetVisualDescendants().OfType<Slider>().Single(slider => slider.Width == 96);

    private static IEnumerable<Button> Buttons(NowPlayingBar bar) =>
        bar.GetVisualDescendants().OfType<Button>();

    private static string? Name(Control control) => AutomationProperties.GetName(control);

    private static void Press(Button button) =>
        button.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
}
