using uniffi.bae_bridge;

namespace Bae.Windows;

internal static class BridgeDisplay
{
    /// <summary>
    /// The clock label for a raw millisecond duration ("3:07", "1:12:34"), or an
    /// empty string when there is nothing to label — no duration, or a negative
    /// one. Core decides the label's fields; <see cref="Loc.Clock"/> renders them.
    /// </summary>
    internal static string Clock(long? milliseconds) =>
        Render(BaeBridgeMethods.BridgeClock(milliseconds));

    /// <summary>The clock label for an unsigned duration, as playback reports it.</summary>
    internal static string Clock(ulong milliseconds) =>
        Clock(checked((long)milliseconds));

    /// <summary>
    /// The remaining clock label from a position within a duration ("-1:23"). Core
    /// clamps the countdown at the end of the track.
    /// </summary>
    internal static string RemainingClock(ulong positionMs, ulong durationMs) =>
        Render(BaeBridgeMethods.BridgeRemainingClock(positionMs, durationMs));

    private static string Render(BridgeDurationClock? clock) =>
        clock is null
            ? string.Empty
            : Loc.Clock(clock.Negative, clock.Hours, clock.Minutes, clock.Seconds);

    internal static string LocalizedLine(BridgeException exception) =>
        exception switch
        {
            BridgeException.NotFound notFound => Loc.Core(
                BaeBridgeMethods.BridgeEntityNotFoundKey(notFound.entity)),
            BridgeException.Diagnostic diagnostic => Loc.Core(
                BaeBridgeMethods.BridgeErrorCategoryKey(diagnostic.category)),
            _ => Loc.Core("core.error.category.internal"),
        };

    internal static string LocalizedLine(BridgeLookupFailure failure)
    {
        var key = BaeBridgeMethods.BridgeLookupFailureKey(failure);
        if (key is null)
        {
            return Loc.Core("core.lookup.failure.diagnostic");
        }

        return failure is BridgeLookupFailure.Provider { Status: { } status }
            ? Loc.Core(key, "status", status)
            : Loc.Core(key);
    }

    internal static string LocalizedLine(BridgePlaybackErrorReason reason)
    {
        var key = BaeBridgeMethods.BridgePlaybackErrorReasonKey(reason);
        if (key is not null)
        {
            return Loc.Core(key);
        }

        return reason is BridgePlaybackErrorReason.Diagnostic diagnostic
            ? LocalizedLine(diagnostic.Error)
            : Loc.Core("core.error.category.internal");
    }

    internal static string LocalizedLine(BridgeInvalidReason reason)
    {
        var key = BaeBridgeMethods.BridgeInvalidReasonKey(reason);
        return reason switch
        {
            BridgeInvalidReason.CorruptAudioFile file => Loc.Core(key, "path", file.Path),
            BridgeInvalidReason.CorruptImage image => Loc.Core(key, "path", image.Path),
            BridgeInvalidReason.CueParseFailed cue => Loc.Core(key, "path", cue.Path),
            _ => Loc.Core(key),
        };
    }

    // The wire provider tag (google_drive / dropbox / onedrive / s3) as a name to
    // show the user. Unknown tags pass through unchanged.
    internal static string ProviderDisplayName(string provider) => provider switch
    {
        "google_drive" => Loc.Chrome("cloud.provider.google_drive"),
        "dropbox" => Loc.Chrome("cloud.provider.dropbox"),
        "onedrive" => Loc.Chrome("cloud.provider.onedrive"),
        "s3" => Loc.Chrome("cloud.provider.s3"),
        _ => provider,
    };

    internal static string ProviderDisplayName(BridgeCloudProvider provider)
    {
        var key = NativeBae.CloudProviderLabelKey(provider);
        if (key is not null)
        {
            return Loc.Core(key);
        }

        return provider switch
        {
            BridgeCloudProvider.GoogleDrive => Loc.Chrome("cloud.provider.google_drive"),
            BridgeCloudProvider.Dropbox => Loc.Chrome("cloud.provider.dropbox"),
            BridgeCloudProvider.OneDrive => Loc.Chrome("cloud.provider.onedrive"),
            BridgeCloudProvider.CloudKit => Loc.Chrome("cloud.provider.icloud"),
            _ => provider.ToString(),
        };
    }
}
