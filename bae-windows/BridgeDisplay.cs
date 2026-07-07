using uniffi.bae_bridge;

namespace Bae.Windows;

internal static class BridgeDisplay
{
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
}
