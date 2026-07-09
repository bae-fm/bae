using System;
using System.Threading;
using System.Threading.Tasks;
using Velopack;
using Velopack.Sources;

namespace Bae.Windows;

/// <summary>
/// The thin Velopack wrapper behind the settings "updates" section: it checks
/// the GitHub release feed, downloads a staged package, and applies it on
/// restart. All decision logic lives in <see cref="UpdateFlowState"/> /
/// <see cref="UpdateFlowDisplay"/>; this shell only turns Velopack calls into
/// state transitions.
///
/// <see cref="StateChanged"/> is raised on the worker thread that runs a check,
/// so subscribers must marshal to the UI thread.
/// </summary>
internal sealed class UpdateService
{
    // The public repository's GitHub releases are the feed. Anonymous access is
    // one API call per check; the installed package's channel selects its own
    // releases.<channel>.json, so no edition branching is needed here. Static:
    // IsInstalled is also the Velopack-install gate App.OnLaunched asserts the
    // protocol registration behind, before a MainWindow (and its UpdateService)
    // exists.
    private static readonly UpdateManager Manager = new(
        new GithubSource("https://github.com/bae-fm/bae", null, false));

    // The checked-and-downloaded update awaiting apply-on-restart, set once a
    // check reaches Ready. null until then.
    private UpdateInfo? _pending;

    // One check at a time: the background check at launch and a manual check from
    // settings can't overlap. 0 = idle, 1 = a check is running.
    private int _inFlight;

    internal UpdateFlowState State { get; private set; } = new UpdateFlowState.Idle();

    internal event Action<UpdateFlowState>? StateChanged;

    /// <summary>Whether this process is a Velopack install (as opposed to a dev
    /// run or a loose-zip copy). Gates every affordance: the throwing Velopack
    /// calls are only valid on an install.</summary>
    internal bool IsAvailable => Manager.IsInstalled;

    /// <summary>Static form of <see cref="IsAvailable"/> for callers that run
    /// before a <see cref="UpdateService"/> instance exists — the protocol
    /// registration assert in <c>App.OnLaunched</c>, which runs before the
    /// <see cref="MainWindow"/> that owns one.</summary>
    internal static bool IsInstalled => Manager.IsInstalled;

    /// <summary>The installed package version, or null when this isn't an
    /// install.</summary>
    internal string? InstalledVersion =>
        Manager.IsInstalled ? Manager.CurrentVersion?.ToString() : null;

    private void SetState(UpdateFlowState state)
    {
        State = state;
        StateChanged?.Invoke(state);
    }

    /// <summary>Launch-time silent check: if an update exists, download and stage
    /// it to apply on the next exit, landing in <c>Ready</c>. Any failure
    /// (offline, unreachable feed, checksum mismatch) is logged and leaves the
    /// state <c>Idle</c> — a background check never surfaces UI. No-op off an
    /// install.</summary>
    internal async Task CheckInBackgroundAsync()
    {
        if (!IsAvailable || Interlocked.CompareExchange(ref _inFlight, 1, 0) != 0)
        {
            return;
        }

        try
        {
            var info = await Manager.CheckForUpdatesAsync();
            if (info is null)
            {
                return;
            }

            await Manager.DownloadUpdatesAsync(info);
            _pending = info;
            Manager.WaitExitThenApplyUpdates(info.TargetFullRelease, silent: true, restart: false);
            SetState(new UpdateFlowState.Ready(info.TargetFullRelease.Version.ToString()));
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Background update check failed.", exception);
        }
        finally
        {
            Interlocked.Exchange(ref _inFlight, 0);
        }
    }

    /// <summary>Manual check from settings: <c>Checking</c> → <c>UpToDate</c> |
    /// <c>Downloading</c> (progress) → <c>Ready</c>, or <c>Failed</c> on any
    /// error (logged; clicking again retries). No-op off an install.</summary>
    internal async Task CheckAsync()
    {
        if (!IsAvailable)
        {
            return;
        }

        if (Interlocked.CompareExchange(ref _inFlight, 1, 0) != 0)
        {
            // A launch-time background check is still running; it owns the flow.
            BaeDiagnostics.Logger.Debug("Manual update check skipped: a check is already in flight.");
            return;
        }

        try
        {
            SetState(new UpdateFlowState.Checking());
            var info = await Manager.CheckForUpdatesAsync();
            if (info is null)
            {
                SetState(new UpdateFlowState.UpToDate());
                return;
            }

            await Manager.DownloadUpdatesAsync(
                info, percent => SetState(new UpdateFlowState.Downloading(percent)));
            _pending = info;
            SetState(new UpdateFlowState.Ready(info.TargetFullRelease.Version.ToString()));
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Update check failed.", exception);
            SetState(new UpdateFlowState.Failed());
        }
        finally
        {
            Interlocked.Exchange(ref _inFlight, 0);
        }
    }

    /// <summary>Apply the staged update and relaunch. This exits the process, so
    /// the caller must persist playback state first.</summary>
    internal void ApplyAndRestart() => Manager.ApplyUpdatesAndRestart(_pending!.TargetFullRelease);
}
