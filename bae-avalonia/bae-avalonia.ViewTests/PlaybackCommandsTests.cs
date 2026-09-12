using System;
using System.Collections.Generic;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The playback command set: what each action sends to core given what the
/// stores currently hold. Every command names the state it wants, so these
/// cover the derivation — play/pause picking resume or pause, mute naming the
/// opposite of what is current, repeat asking core which mode follows — rather
/// than the transport itself, which is core's.
/// </summary>
public sealed class PlaybackCommandsTests
{
    [Fact]
    public void PlayPausePausesWhatIsPlaying()
    {
        var sent = new List<string>();
        var store = new PlaybackStore(new QueueService(), _ => { });
        store.ApplyPlaying("album", "track", "Title", "Artist", null);
        var commands = Build(new PlaybackService
        {
            Pause = () => Record(sent, "pause"),
            Resume = () => Record(sent, "resume"),
        }, store);

        commands.PlayPause();

        Assert.Equal(new[] { "pause" }, sent);
    }

    [Fact]
    public void PlayPauseResumesWhatIsPaused()
    {
        var sent = new List<string>();
        var store = new PlaybackStore(new QueueService(), _ => { });
        store.ApplyPaused("album", "track", "Title", "Artist", null, new BridgePlaybackPauseReason.Manual());
        var commands = Build(new PlaybackService
        {
            Pause = () => Record(sent, "pause"),
            Resume = () => Record(sent, "resume"),
        }, store);

        commands.PlayPause();

        Assert.Equal(new[] { "resume" }, sent);
    }

    /// <summary>Stopped empties the now-playing slot, so there is nothing to
    /// resume and the command sends nothing. The service's stubs would throw if
    /// it did.</summary>
    [Fact]
    public void PlayPauseSendsNothingWhileStopped()
    {
        var store = new PlaybackStore(new QueueService(), _ => { });
        store.ApplyStopped();
        var commands = Build(new PlaybackService(), store);

        commands.PlayPause();
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void MuteNamesTheOppositeOfWhatIsCurrent(bool muted)
    {
        bool? asked = null;
        var store = new PlaybackStore(new QueueService(), _ => { });
        store.ApplyMute(muted);
        var commands = Build(new PlaybackService
        {
            SetMuted = target =>
            {
                asked = target;
                return true;
            },
        }, store);

        commands.ToggleMute();

        Assert.Equal(!muted, asked);
    }

    [Fact]
    public void CycleRepeatModeSetsTheModeCoreSaysFollowsTheCurrentOne()
    {
        BridgeRepeatMode? from = null;
        BridgeRepeatMode? set = null;
        var store = new PlaybackStore(new QueueService(), _ => { });
        store.ApplyRepeat(BridgeRepeatMode.Context);
        var commands = Build(new PlaybackService
        {
            NextRepeatMode = current =>
            {
                from = current;
                return BridgeRepeatMode.Track;
            },
            SetRepeatMode = mode =>
            {
                set = mode;
                return true;
            },
        }, store);

        commands.CycleRepeatMode();

        Assert.Equal(BridgeRepeatMode.Context, from);
        Assert.Equal(BridgeRepeatMode.Track, set);
    }

    [Fact]
    public void ARepeatModeIsSetAbsolutely()
    {
        BridgeRepeatMode? set = null;
        var commands = Build(new PlaybackService
        {
            SetRepeatMode = mode =>
            {
                set = mode;
                return true;
            },
        }, new PlaybackStore(new QueueService(), _ => { }));

        commands.SetRepeatMode(BridgeRepeatMode.Off);

        Assert.Equal(BridgeRepeatMode.Off, set);
    }

    /// <summary>An empty library has nothing to shuffle. The command reports
    /// itself unavailable and sends nothing — the fail-loud stub proves it.</summary>
    [Fact]
    public void ShuffleLibraryIsUnavailableWithNoAlbums()
    {
        var commands = Build(new PlaybackService(), new PlaybackStore(new QueueService(), _ => { }), albumCount: 0);

        Assert.False(commands.CanShuffleLibrary);
        commands.ShuffleLibrary();
    }

    [Fact]
    public void ShuffleLibraryPlaysOnceTheLibraryHasAnAlbum()
    {
        var shuffled = 0;
        var commands = Build(
            new PlaybackService { PlayLibraryShuffled = () => { shuffled++; return true; } },
            new PlaybackStore(new QueueService(), _ => { }),
            albumCount: 1);

        Assert.True(commands.CanShuffleLibrary);
        commands.ShuffleLibrary();

        Assert.Equal(1, shuffled);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void PauseBetweenSidesWritesTheOppositeOfTheSettingsMirror(bool current)
    {
        bool? written = null;
        var settings = SettingsMirror(pauseBetweenSides: current);
        var commands = Build(
            new PlaybackService
            {
                SetPauseBetweenSides = target =>
                {
                    written = target;
                    return (true, null);
                },
            },
            new PlaybackStore(new QueueService(), _ => { }),
            settings: settings);

        Assert.Equal(current, commands.PauseBetweenSides);
        commands.TogglePauseBetweenSides();

        Assert.Equal(!current, written);
    }

    /// <summary>A synced preference write can come back with an error line; it
    /// goes to the same error surface the settings window uses.</summary>
    [Fact]
    public void AFailedPauseBetweenSidesWriteSurfacesItsError()
    {
        var shown = new List<string>();
        var commands = Build(
            new PlaybackService { SetPauseBetweenSides = _ => (true, "could not save") },
            new PlaybackStore(new QueueService(), _ => { }),
            settings: SettingsMirror(pauseBetweenSides: false),
            showError: shown.Add);

        commands.TogglePauseBetweenSides();

        Assert.Equal(new[] { "could not save" }, shown);
    }

    /// <summary>A write that never reached a handle has no error to report —
    /// the library closed under it, which the trace records and nobody is
    /// shown.</summary>
    [Fact]
    public void APauseBetweenSidesWriteWithNoHandleShowsNothing()
    {
        var shown = new List<string>();
        var commands = Build(
            new PlaybackService { SetPauseBetweenSides = _ => (false, null) },
            new PlaybackStore(new QueueService(), _ => { }),
            settings: SettingsMirror(pauseBetweenSides: false),
            showError: shown.Add);

        commands.TogglePauseBetweenSides();

        Assert.Empty(shown);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void RestoreOnLaunchWritesTheOppositeOfWhatIsStored(bool stored)
    {
        bool? written = null;
        var commands = Build(
            new PlaybackService(),
            new PlaybackStore(new QueueService(), _ => { }),
            readRestoreOnLaunch: () => stored,
            writeRestoreOnLaunch: target => written = target);

        Assert.Equal(stored, commands.RestoreOnLaunch);
        commands.ToggleRestoreOnLaunch();

        Assert.Equal(!stored, written);
    }

    private static bool Record(List<string> sent, string command)
    {
        sent.Add(command);
        return true;
    }

    private static SettingsStore SettingsMirror(bool pauseBetweenSides)
    {
        var store = new SettingsStore(new SettingsService
        {
            GetSettings = () => (true, new Settings { PauseBetweenSides = pauseBetweenSides }),
        });
        store.Reload();
        return store;
    }

    private static PlaybackCommands Build(
        PlaybackService playback,
        PlaybackStore store,
        SettingsStore? settings = null,
        int albumCount = 1,
        Func<bool>? readRestoreOnLaunch = null,
        Action<bool>? writeRestoreOnLaunch = null,
        Action<string>? showError = null) =>
        new(
            playback,
            store,
            settings ?? SettingsMirror(pauseBetweenSides: false),
            () => albumCount,
            readRestoreOnLaunch ?? (() => false),
            writeRestoreOnLaunch ?? (_ => { }),
            showError ?? (_ => { }));
}
