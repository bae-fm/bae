using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.Storage;
using Windows.System;

namespace Bae.Windows;

// MainWindow: opening, switching, and tearing down the active library, the
// activation-intent handling, and the sync/banner rendering. Split out of
// MainWindow.xaml.cs unchanged.
public sealed partial class MainWindow : Window
{
    // Create a new library, reporting failure through the caller's surface (the
    // welcome status line, or the library manager's). Returns the new id, or null
    // on failure. Callers diverge only in how they open it.
    private string? CreateLibraryOrReport(Action<string> reportError)
    {
        try
        {
            return NativeBae.CreateLibrary();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to create library.", exception);
            reportError(exception.Message);
            return null;
        }
    }

    // The libraries discovered on this device. Empty when discovery fails or none
    // exist; callers pick the active one, or list them.
    private List<BridgeLibrary> LoadLibraries()
    {
        try
        {
            return NativeBae.Libraries();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to discover libraries.", exception);
            StatusText.Text = exception.Message;
            return new List<BridgeLibrary>();
        }
    }

    private void LoadLibrary()
    {
        // A library whose config.yaml will not load is listed (so the user can see
        // it is there), but it cannot be opened — never auto-open into one.
        var libraries = LoadLibraries().Where(candidate => candidate.Error is null).ToList();
        var library = libraries.FirstOrDefault(candidate => candidate.IsActive)
            ?? libraries.FirstOrDefault();
        if (library is null)
        {
            _welcomeView.Show();
            // Settled: no library exists to hand a launch-time intent to, so
            // it applies now as the no-op HandleActivationIntent logs.
            SettleInitialLibraryOpen();
            return;
        }

        OpenLibrary(library.Id);
    }

    private void OpenLibrary(string libraryId)
    {
        switch (_session.OpenHandle(libraryId))
        {
            case OpenHandleResult.Failed:
                StatusText.Text = Loc.Chrome("library.open_failed");
                SettleInitialLibraryOpen();
                return;
            case OpenHandleResult.NeedsUnlock:
                // Encrypted library whose key isn't on this device: the session
                // freed the handle it made; prompt for the key rather than show a
                // half-open library. Unlocking re-opens with sync online, which
                // settles the latch below through this same method; until then
                // a pending launch intent stays latched.
                _ = _unlockDialog.Show(libraryId);
                return;
        }

        // Committed to showing this library: drop the welcome chooser if it's up.
        // Done here (not at the call sites) so a failed open or an unlock detour
        // above leaves the welcome in place rather than stranding the user.
        _welcomeView.Dismiss();

        LoadCurrentBrowserMode();
        _nowPlayingBar.SeedVolume();
        // Seed the config mirror: the now-playing bar reads its time-label mode
        // from it, so it must be populated before the first position tick.
        _settings.Reload();
        _nowPlayingBar.RefreshTimeLabelMode();
        _sync.Refresh();
        _session.Subscribe();
        // Host-originated telemetry: the library screen opened, through the
        // standalone sink. Infallible.
        NativeBae.ReportScreen(BaeDiagnostics.Handle, BridgeScreen.Library);
        SettleInitialLibraryOpen();
    }

    // Set by App.OnLaunched once the window exists. Applies immediately when
    // the initial library-open attempt has already settled (the common case:
    // LoadLibrary runs synchronously in the constructor, before this is
    // called); otherwise latches until SettleInitialLibraryOpen runs.
    internal void SetPendingLaunchIntent(ActivationIntent intent)
    {
        if (_initialLibraryOpenSettled)
        {
            _ = HandleActivationIntent(intent);
        }
        else
        {
            _pendingLaunchIntent = intent;
        }
    }

    // Idempotent: called at every terminal point of the launch-time
    // LoadLibrary/OpenLibrary flow (no library found, a failed open, or a
    // successful open), including the one that runs after an async unlock
    // resolves. Only the first call after a pending intent was set has an
    // effect; later calls (a manual library switch, for instance) find nothing
    // latched.
    private void SettleInitialLibraryOpen()
    {
        _initialLibraryOpenSettled = true;
        if (_pendingLaunchIntent is { } intent)
        {
            _pendingLaunchIntent = null;
            _ = HandleActivationIntent(intent);
        }
    }

    // Turn a parsed activation intent (the folder verb, a dropped-on-the-exe
    // folder, or bae://import) into the same UI action OnWindowDrop runs for a
    // dropped folder.
    internal async System.Threading.Tasks.Task HandleActivationIntent(ActivationIntent intent)
    {
        switch (intent)
        {
            case ActivationIntent.ImportFolder importFolder:
                if (CurrentHandleOrNull() == null)
                {
                    // No open library (welcome screen or an unresolved unlock):
                    // a no-op, matching the macOS handler's folder add.
                    BaeDiagnostics.Logger.Info(
                        "Ignored a folder-import activation: no library is open.");
                    return;
                }
                await ImportFolder(importFolder.Path);
                return;
            default:
                throw new ArgumentOutOfRangeException(nameof(intent), intent, "Unknown activation intent");
        }
    }

    // Bring the window to the front for a redirected activation (a second
    // launch while bae is already running): restore it first if minimized.
    // The OS may downgrade this to a taskbar flash when foreground rights are
    // denied to a background process — accepted, not worked around.
    internal void BringToForeground()
    {
        if (AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized } presenter)
        {
            presenter.Restore();
        }

        SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
    }

    // Render the toolbar sync indicator and the sync banner from the sync store's
    // current state (subscribed to its Changed).
    private void RenderSyncStatus()
    {
        if (_sync.ErrorText is null)
        {
            SyncBanner.IsOpen = false;
        }
        else
        {
            var reconnect = new Button { Content = Loc.Chrome("sync.reconnect") };
            reconnect.Click += (_, _) => WithCurrentHandle(NativeBae.TriggerSync);
            SyncBanner.Severity = InfoBarSeverity.Error;
            SyncBanner.Title = Loc.Chrome("sync.error_title");
            SyncBanner.Message = _sync.ErrorText;
            SyncBanner.ActionButton = reconnect;
            SyncBanner.IsOpen = true;
        }

        switch (_sync.Indicator)
        {
            case BridgeSyncIndicator.Error:
                SyncIndicator.Text = Loc.Chrome("sync.error_title");
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                break;
            case BridgeSyncIndicator.Syncing:
                SyncIndicator.Text = Loc.Chrome("sync.syncing");
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                break;
            case BridgeSyncIndicator.Synced:
                SyncIndicator.Text = Loc.Chrome("sync.synced", "time", _sync.LastSyncTime);
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                break;
            case BridgeSyncIndicator.Idle:
                SyncIndicator.Text = string.Empty;
                break;
        }
    }

    // Render the shell error banner from the shell store (subscribed to Changed).
    private void RenderBanner()
    {
        Banner.Severity = _shell.BannerSeverity;
        Banner.Title = _shell.BannerTitle;
        Banner.Message = _shell.BannerMessage;
        Banner.ActionButton = null;
        Banner.IsOpen = _shell.BannerIsOpen;
    }

    // Switch the active library: persist the current one's playback state, tear
    // down its handle and view state, then open the target. generated bridge init records the
    // target as the active library once it opens unlocked; a locked target lands
    // on the unlock prompt. Used for switching to an existing library and for a
    // freshly created one.
    private async System.Threading.Tasks.Task SwitchLibrary(string libraryId)
    {
        await TearDownLibrary();
        OpenLibrary(libraryId);
    }

    // Tear down the open library: shut down and free its handle, and reset every
    // piece of per-library view state so nothing from it bleeds into the next
    // library or the welcome chooser. Leaves the window with no library open.
    private async System.Threading.Tasks.Task TearDownLibrary()
    {
        await _session.ShutdownAndFreeCurrentHandle();

        _playback.Reset();
        _nowPlayingBar.Reset();
        _queuePane.Hide();
        _import.Reset();
        CollapseAlbumExpansion();
        _browser.Reset();
        _albumSelection.Clear();
        _transferProgress.Reset();
        SearchBox.Text = string.Empty;
        StatusText.Text = string.Empty;
        ShuffleLibraryItem.IsEnabled = false;
        _mediaControls.Deactivate();
        // The banners report the old library's sync / playback errors; clear them
        // so they don't describe state the next library (or none) doesn't have.
        _shell.ClearBanner();
        _sync.Reset();
    }

    // Close the open library and return to the welcome chooser, which now lists
    // the libraries on disk so the user can reopen one or create another.
    private async System.Threading.Tasks.Task CloseLibrary()
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        await TearDownLibrary();
        _welcomeView.Show();
    }

    private System.Threading.Tasks.Task ShutdownAndFreeCurrentHandle() =>
        _session.ShutdownAndFreeCurrentHandle();

    private bool WithCurrentHandle(Action<AppHandle> action) =>
        _session.WithCurrentHandle(action);

    private LibraryHandle? CurrentHandleOrNull() =>
        _session.CurrentHandleOrNull();
}
