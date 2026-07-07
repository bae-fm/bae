using System;
using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// The phase of the settings "updates" section, driving what the dialog renders.
/// Pure over plain BCL types with no WinUI or Velopack dependency, so it compiles
/// in the test project on any host; the Velopack calls that produce these states
/// live in <see cref="UpdateService"/>.
/// </summary>
internal abstract record UpdateFlowState
{
    internal sealed record Idle : UpdateFlowState;
    internal sealed record Checking : UpdateFlowState;
    internal sealed record UpToDate : UpdateFlowState;

    /// <summary>Download progress. Clamped to 0..100 at construction so a
    /// provider that reports outside the range can't push an invalid percent to
    /// the display.</summary>
    internal sealed record Downloading(int Percent) : UpdateFlowState
    {
        internal int Percent { get; } = Math.Clamp(Percent, 0, 100);
    }

    /// <summary>An update is downloaded and staged; the app applies it on the
    /// next exit. <paramref name="Version"/> is the raw package version string.</summary>
    internal sealed record Ready(string Version) : UpdateFlowState;

    internal sealed record Failed : UpdateFlowState;
}

/// <summary>
/// Pure rendering decisions for the settings "updates" section, derived from an
/// <see cref="UpdateFlowState"/>. The dialog reads a localized status key,
/// whether the check button is enabled, and whether the restart button shows;
/// none of it touches WinUI, so it is exercised directly by the unit tests.
/// </summary>
internal static class UpdateFlowDisplay
{
    /// <summary>The status line's chrome key and its MessageFormat arguments, or
    /// null when the state has no status line (idle).</summary>
    internal static (string Key, IReadOnlyDictionary<string, object?>? Args)? StatusFor(UpdateFlowState state) =>
        state switch
        {
            UpdateFlowState.Idle => null,
            UpdateFlowState.Checking => ("settings.updates.checking", null),
            UpdateFlowState.UpToDate => ("settings.updates.up_to_date", null),
            UpdateFlowState.Downloading downloading =>
                ("settings.updates.downloading",
                    new Dictionary<string, object?> { ["percent"] = downloading.Percent }),
            UpdateFlowState.Ready ready =>
                ("settings.updates.ready",
                    new Dictionary<string, object?> { ["version"] = VersionDisplay(ready.Version) }),
            UpdateFlowState.Failed => ("settings.updates.failed", null),
            _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown update flow state"),
        };

    /// <summary>Whether the check button accepts a click. A check already in
    /// flight (checking or downloading) disables it.</summary>
    internal static bool CheckEnabled(UpdateFlowState state) =>
        state is not (UpdateFlowState.Checking or UpdateFlowState.Downloading);

    /// <summary>Whether the restart button shows: only once an update is staged
    /// and ready to apply.</summary>
    internal static bool RestartVisible(UpdateFlowState state) => state is UpdateFlowState.Ready;

    /// <summary>The version string shown to the user, normalized to the release
    /// line's form: a full <c>0.5.0</c> package version drops its trailing zero
    /// patch to read as <c>0.5</c> (matching the <c>windows-v0.5</c> tag), while
    /// a non-zero patch (<c>0.5.1</c>) and an already-short version (<c>0.5</c>)
    /// pass through unchanged.</summary>
    internal static string VersionDisplay(string semver)
    {
        var parts = semver.Split('.');
        return parts.Length == 3 && parts[2] == "0"
            ? $"{parts[0]}.{parts[1]}"
            : semver;
    }
}
