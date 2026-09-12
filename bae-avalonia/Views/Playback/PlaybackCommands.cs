using System;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The playback actions the app's controls invoke. A menu item and its keyboard
/// shortcut call the same method here, so no surface re-derives what a press
/// means.
///
/// Every command names the state it wants: it reads the current value from the
/// stores and writes the absolute target (<c>SetMuted(!IsMuted)</c>), rather
/// than asking a store to flip itself. Core is the only thing that holds
/// playback state, so a control that flipped its own copy would drift from it.
///
/// The preference reads and writes are handed in rather than reached for, so
/// this layer decides and never touches the config or the disk itself.
/// </summary>
internal sealed class PlaybackCommands
{
    private readonly PlaybackService _playback;
    private readonly PlaybackStore _store;
    private readonly SettingsStore _settings;
    private readonly Func<int> _albumCount;
    private readonly Func<bool> _readRestoreOnLaunch;
    private readonly Action<bool> _writeRestoreOnLaunch;
    private readonly Action<string> _showError;

    public PlaybackCommands(
        PlaybackService playback,
        PlaybackStore store,
        SettingsStore settings,
        Func<int> albumCount,
        Func<bool> readRestoreOnLaunch,
        Action<bool> writeRestoreOnLaunch,
        Action<string> showError)
    {
        _playback = playback;
        _store = store;
        _settings = settings;
        _albumCount = albumCount;
        _readRestoreOnLaunch = readRestoreOnLaunch;
        _writeRestoreOnLaunch = writeRestoreOnLaunch;
        _showError = showError;
    }

    /// <summary>Pause what is playing, or resume what is paused. Stopped means
    /// the now-playing slot is empty, so there is nothing to resume and the
    /// command does nothing.</summary>
    public void PlayPause()
    {
        switch (_store.PlayState)
        {
            case TransportPlayState.Playing:
                Dispatch("pause", _playback.Pause);
                break;
            case TransportPlayState.Paused:
                Dispatch("resume", _playback.Resume);
                break;
            case TransportPlayState.Stopped:
                break;
        }
    }

    public void NextTrack() => Dispatch("next track", _playback.NextTrack);

    public void PreviousTrack() => Dispatch("previous track", _playback.PreviousTrack);

    /// <summary>Mute what is audible, or unmute what is muted.</summary>
    public void ToggleMute() => Dispatch("mute", () => _playback.SetMuted(!_store.IsMuted));

    public BridgeRepeatMode RepeatMode => _store.RepeatMode;

    /// <summary>Step to the mode that follows the current one. Which mode
    /// follows which is core's answer, asked for rather than reproduced.</summary>
    public void CycleRepeatMode() =>
        Dispatch("cycle repeat mode", () => _playback.SetRepeatMode(_playback.NextRepeatMode(_store.RepeatMode)));

    public void SetRepeatMode(BridgeRepeatMode mode) =>
        Dispatch("set repeat mode", () => _playback.SetRepeatMode(mode));

    /// <summary>An empty library has nothing to shuffle, so the control that
    /// offers it is unavailable rather than doing nothing when pressed.</summary>
    public bool CanShuffleLibrary => _albumCount() > 0;

    public void ShuffleLibrary()
    {
        if (CanShuffleLibrary)
        {
            Dispatch("shuffle library", _playback.PlayLibraryShuffled);
        }
    }

    /// <summary>Whether playback pauses between an album's sides. A synced
    /// preference read from the settings mirror, which is null until the first
    /// snapshot arrives.</summary>
    public bool PauseBetweenSides => _settings.Current?.PauseBetweenSides == true;

    public void TogglePauseBetweenSides()
    {
        var (current, error) = _playback.SetPauseBetweenSides(!PauseBetweenSides);
        if (current && error is not null)
        {
            _showError(error);
        }
    }

    /// <summary>Whether quitting saves the current track, position, queue, and
    /// volume for the next launch to restore. Device-local, so writing it
    /// cannot fail the way a synced preference can.</summary>
    public bool RestoreOnLaunch => _readRestoreOnLaunch();

    public void ToggleRestoreOnLaunch() => _writeRestoreOnLaunch(!RestoreOnLaunch);

    // A transport command returns whether it reached a current handle. It can
    // miss one by racing a library teardown — the press landed after the session
    // was cleared — which is not an error to show anyone, only something to see
    // in a trace.
    private static void Dispatch(string command, Func<bool> run)
    {
        if (!run())
        {
            BaeDiagnostics.Logger.Debug($"Dropped playback command {command}: no open library handle.");
        }
    }
}
