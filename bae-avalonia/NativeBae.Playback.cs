using System.Linq;
using System.Text.Json;
using uniffi.bae_bridge;


namespace Bae.Desktop;

internal static partial class NativeBae
{
    internal static void PlayRelease(AppHandle handle, string releaseId, long startTrackIndex, bool shuffle) =>
        handle.PlayRelease(releaseId, startTrackIndex < 0 ? null : checked((uint)startTrackIndex), shuffle);

    // The album grid's bulk play: fills the context lane with every targeted
    // album's primary release, in the given order. Up Next is left alone and
    // still drains first.
    internal static void PlayReleases(AppHandle handle, IReadOnlyList<string> releaseIds) =>
        handle.PlayReleases(releaseIds.ToArray());

    internal static void PlayLibraryShuffled(AppHandle handle) => handle.PlayLibraryShuffled();

    internal static void Pause(AppHandle handle) => handle.Pause();

    internal static void Resume(AppHandle handle) => handle.Resume();

    // End playback and empty the now-playing slot, as opposed to pausing it. The
    // in-app transport has no stop button; the OS now-playing surfaces do.
    internal static void Stop(AppHandle handle) => handle.Stop();

    internal static void SeekByRatio(AppHandle handle, double ratio) => handle.SeekByRatio(ratio);

    internal static void PreviewSeekByRatio(AppHandle handle, double ratio) => handle.PreviewSeekByRatio(ratio);

    internal static void SetVolume(AppHandle handle, float volume) => handle.SetVolume(volume);

    internal static void SetMuted(AppHandle handle, bool muted) => handle.SetMuted(muted);

    internal static float GetVolume(AppHandle handle) => Await(() => handle.GetVolume());

    // -- Cast --

    internal static IReadOnlyList<BridgeCastDevice> GetCastDevices(AppHandle handle) =>
        handle.GetCastDevices();

    internal static void StartCastDiscovery(AppHandle handle) => handle.StartCastDiscovery();

    internal static void StopCastDiscovery(AppHandle handle) => handle.StopCastDiscovery();

    // Returns a localized error line on failure (connect/serving), or null on
    // success.
    internal static string? CastTo(AppHandle handle, string deviceId) =>
        CaptureError(() => handle.CastTo(deviceId));

    internal static void StopCasting(AppHandle handle) => handle.StopCasting();

    /// <summary>Whether casting is available at all. Turning it off stops discovery
    /// and disconnects any session in flight; the write fires a config
    /// invalidation, which is what hides the cast control.</summary>
    internal static string? SetCastEnabled(AppHandle handle, bool enabled) =>
        CaptureError(() => handle.SetCastEnabled(enabled));

    internal static void SetRepeatMode(AppHandle handle, BridgeRepeatMode mode) => handle.SetRepeatMode(mode);

    /// <summary>The mode the repeat button steps to next. Playback only accepts an
    /// absolute mode, so the button computes its target — but the cycle's order is
    /// core's, not this app's.</summary>
    internal static BridgeRepeatMode NextRepeatMode(BridgeRepeatMode mode) =>
        BaeBridgeMethods.BridgeNextRepeatMode(mode);

    internal static void SetShuffle(AppHandle handle, bool on) => handle.SetShuffle(on);

    internal static void Next(AppHandle handle) => handle.NextTrack();

    internal static void Previous(AppHandle handle) => handle.PreviousTrack();

    internal static void QueueSkipTo(AppHandle handle, string entryId) => handle.SkipToEntry(entryId);

    internal static void QueueRemove(AppHandle handle, string entryId) => handle.RemoveEntry(entryId);

    internal static void QueueReorder(AppHandle handle, string entryId, string? beforeEntryId) =>
        handle.ReorderEntry(entryId, beforeEntryId);

    internal static void QueueClearUpNext(AppHandle handle) => handle.ClearUpNext();

    internal static void QueueClearPlayingFrom(AppHandle handle) => handle.ClearPlayingFrom();

    // One page of the context's upcoming tail past the window QueueUpdated
    // already carries. offset 0 is the first not-yet-played entry after the
    // current track — the same coordinate space as BridgePlaybackContext.Upcoming.
    internal static (BridgeQueueUpcomingPage? Page, string? Error) QueueUpcomingPage(AppHandle handle, uint offset, uint limit) =>
        CaptureBridgeValue(() => Await(() => handle.GetQueueUpcomingPage(offset, limit)));

    internal static void AddReleaseToQueue(AppHandle handle, string releaseId) => handle.AddReleaseToQueue(releaseId);

    internal static void AddReleaseNext(AppHandle handle, string releaseId) => handle.AddReleaseNext(releaseId);

    internal static string? AddToQueue(AppHandle handle, IReadOnlyList<string> trackIds) =>
        CaptureError(() => handle.AddToQueue(trackIds.ToArray()));

    internal static string? AddNext(AppHandle handle, IReadOnlyList<string> trackIds) =>
        CaptureError(() => handle.AddNext(trackIds.ToArray()));

    // Expand a list of album or track ids to track ids (album ids resolve to the
    // primary release's tracks), for a queue insert that carries a drag payload.
    internal static (List<string>? Value, string? Error) ResolveToTrackIds(AppHandle handle, IReadOnlyList<string> ids) =>
        CaptureBridgeValue(() => Await(() => handle.ResolveToTrackIds(ids.ToArray())).ToList());

    // Insert tracks into the manual lane at index (core clamps to the lane length).
    internal static void InsertInQueue(AppHandle handle, IReadOnlyList<string> trackIds, int index) =>
        handle.InsertInQueue(trackIds.ToArray(), checked((uint)index));

    internal static string? DeleteRelease(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(() => handle.DeleteRelease(releaseId)));

    internal static void Shutdown(AppHandle handle) => Await(() => handle.Shutdown());
}
