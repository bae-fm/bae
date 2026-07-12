using System;
using uniffi.bae_bridge;

namespace Bae.Windows;

internal static class RepeatModeCycle
{
    // The next mode in the repeat button's cycle: Off → Context → Track →
    // Off. UI-owned: core only accepts absolute SetRepeatMode values; the
    // caller computes the target from the mode it renders.
    internal static BridgeRepeatMode Next(this BridgeRepeatMode mode) =>
        mode switch
        {
            BridgeRepeatMode.Off => BridgeRepeatMode.Context,
            BridgeRepeatMode.Context => BridgeRepeatMode.Track,
            BridgeRepeatMode.Track => BridgeRepeatMode.Off,
            _ => throw new ArgumentOutOfRangeException(nameof(mode), mode, "Unknown repeat mode"),
        };
}
