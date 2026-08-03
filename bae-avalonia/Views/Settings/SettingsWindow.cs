using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The settings window: library label, playback preferences, export formats, MCP
// automation, the Subsonic server, the Discogs key, cloud sync (disconnect / S3 /
// OAuth), devices (membership + approve), the recovery code, and updates. A real
// window, like macOS's Settings scene; its own modal host presents the sub-dialogs
// (the preset editor, delete confirm, the approve flow) directly over it. Every read
// and write goes through the app services, never NativeBae. Reads the settings
// through the settings mirror and re-renders when a config invalidation — or an
// in-window connect/disconnect — reloads them; those registrations live only
// while the window is open.
internal sealed partial class SettingsWindow
{
    private readonly AppService _app;

    // The process-wide update service the updates section renders and drives.
    private readonly UpdateService _updates;

    // The coordinator's callbacks: closeToWelcome returns to the welcome chooser
    // (after forgetting the library); switchLibrary re-opens a library (used to land
    // on the unlock prompt after locking the current one); applyUpdateAndRestart is
    // an app-exit path — it tears the session down and relaunches into the staged
    // update, so the coordinator owns it the way it owns quitting.
    private readonly Func<Task> _closeToWelcome;
    private readonly Func<string, Task> _switchLibrary;
    private readonly Func<Task> _applyUpdateAndRestart;

    // The one open settings window; Show re-activates it instead of stacking a
    // second.
    private Window? _window;

    // Per-open state, assigned in Show. The modal host presents the sub-dialogs;
    // the shared error line surfaces action feedback for the config sections; the
    // two guards suppress the change handlers a programmatic re-render fires.
    private ModalHost _modalHost = null!;
    private TextBlock _settingsError = null!;
    private bool _refreshingSettings;
    private bool _discogsBusy;

    public SettingsWindow(
        AppService app,
        UpdateService updates,
        Func<Task> closeToWelcome,
        Func<string, Task> switchLibrary,
        Func<Task> applyUpdateAndRestart)
    {
        _app = app;
        _updates = updates;
        _closeToWelcome = closeToWelcome;
        _switchLibrary = switchLibrary;
        _applyUpdateAndRestart = applyUpdateAndRestart;
    }

    public void Show()
    {
        if (_window is { } open)
        {
            open.Activate();
            return;
        }
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }

        var (current, s) = _app.Settings.GetSettings();
        if (!current)
        {
            return;
        }

        // Host-originated telemetry: the settings screen opened.
        _app.ReportScreen(BridgeScreen.Settings);

        _refreshingSettings = false;
        _discogsBusy = false;
        _modalHost = new ModalHost();
        _settingsError = DialogUi.Danger();

        // Each section registers a renderer that seeds it from the persisted
        // settings and re-drives it on a config re-read; the same list seeds the
        // initial render and drives Refresh.
        var renderers = new List<Action<Settings>>();
        var content = new StackPanel { Spacing = 12, Margin = new Thickness(24, 16) };

        var libraryLabel = SecondaryLabel(string.Empty);
        libraryLabel[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        content.Children.Add(libraryLabel);
        renderers.Add(fresh => libraryLabel.Text = Loc.Chrome("settings.library_label", "name", fresh.LibraryName));

        BuildPlayback(content, renderers);
        BuildCast(content, renderers);
        BuildFormats(content, renderers);
        BuildAutomation(content, renderers);
        BuildSubsonic(content, renderers);
        BuildDiscogs(content, renderers);
        // The shared error line sits after the config sections, as it does on
        // macOS; the cloud section surfaces its own errors inline.
        content.Children.Add(_settingsError);
        BuildCloud(content, renderers);
        BuildMembers(content);
        BuildRecovery(content);
        var unsubscribeUpdates = BuildUpdates(content);
        BuildLibraryLifecycle(content, renderers);

        foreach (var render in renderers)
        {
            render(s);
        }

        var root = new Panel();
        root[!Panel.BackgroundProperty] = new DynamicResourceExtension("BaeBackgroundBrush");
        root.Children.Add(new ScrollViewer { Content = content });
        root.Children.Add(_modalHost);

        var window = new Window
        {
            Title = Loc.Chrome("settings.title"),
            Width = 600,
            Height = 800,
            Content = root,
        };
        _window = window;

        // Re-read the settings into the live controls so a config invalidation — or
        // a connect/disconnect in this window — updates them in place.
        void Refresh()
        {
            if (_app.SettingsStore.Current is not { } fresh)
            {
                return;
            }
            foreach (var render in renderers)
            {
                render(fresh);
            }
        }
        _app.SettingsStore.Changed += Refresh;
        var configRegistration = _app.ProjectionRegistry.Register(
            typeof(BridgeInvalidation.Config), () => _app.SettingsStore.Reload());

        // A key saved while offline lands "unvalidated"; opening settings is a
        // chance to settle it now that there may be connectivity. The core no-ops
        // unless the stored key is actually unvalidated, so call unconditionally.
        _ = _app.Discogs.RevalidateToken();

        window.Closed += (_, _) =>
        {
            _app.SettingsStore.Changed -= Refresh;
            configRegistration.Dispose();
            unsubscribeUpdates?.Invoke();
            _window = null;
        };

        window.Show();
    }

    // ── Playback preferences ─────────────────────────────────────────────────

    private void BuildPlayback(StackPanel content, List<Action<Settings>> renderers)
    {
        var pauseBetweenSides = new CheckBox { Content = Loc.Chrome("settings.playback.pause_between_sides") };
        pauseBetweenSides.IsCheckedChanged += (_, _) =>
        {
            if (_refreshingSettings)
            {
                return;
            }
            ClearSettingsError();
            var enabled = pauseBetweenSides.IsChecked == true;
            var (current, error) = _app.Playback.SetPauseBetweenSides(enabled);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _refreshingSettings = true;
                pauseBetweenSides.IsChecked = !enabled;
                _refreshingSettings = false;
            }
        };
        content.Children.Add(pauseBetweenSides);
        renderers.Add(fresh =>
        {
            _refreshingSettings = true;
            pauseBetweenSides.IsChecked = fresh.PauseBetweenSides;
            _refreshingSettings = false;
        });

        // Whether quitting performs the graceful shutdown that saves the current
        // track, position, queue, and volume for the next launch to restore.
        // Device-local, so no error snap-back and no config-invalidation involvement.
        var restoreOnLaunch = new CheckBox
        {
            Content = Loc.Chrome("settings.playback.restore_on_launch"),
            IsChecked = PersistPlaybackStore.Load(),
        };
        restoreOnLaunch.IsCheckedChanged += (_, _) => PersistPlaybackStore.Save(restoreOnLaunch.IsChecked == true);
        content.Children.Add(restoreOnLaunch);
        content.Children.Add(SecondaryLabel(Loc.Chrome("settings.playback.restore_on_launch_help")));
    }

    // ── Formats / export ─────────────────────────────────────────────────────

    private void BuildFormats(StackPanel content, List<Action<Settings>> renderers)
    {
        var exportSection = new FormatsSettingsSection(
            _app, ShowSettingsError, ClearSettingsError, build => _modalHost.Show(build));
        content.Children.Add(exportSection.View);
        renderers.Add(exportSection.Render);
    }

    // ── Members (devices) ────────────────────────────────────────────────────

    private void BuildMembers(StackPanel content)
    {
        content.Children.Add(SectionLabel(Loc.Chrome("members.title")));
        var membersHost = new StackPanel { Spacing = 8 };
        membersHost.Children.Add(new Spinner { Width = 20, Height = 20 });
        content.Children.Add(membersHost);

        var membersPane = new MembersPane(_app);
        var approveDialog = new ApproveDeviceDialog(_app);

        // The add-device button (owner-only) runs the approve flow over this
        // window's modal host, then refreshes the device list when it finishes.
        async void OnAddDevice()
        {
            await _modalHost.Show(close => approveDialog.Build(close));
            await membersPane.LoadInto(membersHost, OnAddDevice);
        }
        _ = membersPane.LoadInto(membersHost, OnAddDevice);
    }

    // ── Recovery code ────────────────────────────────────────────────────────

    private void BuildRecovery(StackPanel content)
    {
        content.Children.Add(SectionLabel(Loc.Chrome("settings.recovery.title")));
        content.Children.Add(SecondaryLabel(Loc.Chrome("settings.recovery.intro")));

        var recoveryCode = new TextBox
        {
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("monospace"),
            IsVisible = false,
        };
        var show = new Button { Content = Loc.Chrome("settings.recovery.show") };
        show.Click += async (_, _) =>
        {
            var (current, code) = await _app.Sync.GenerateRestoreCode();
            if (!current)
            {
                return;
            }
            recoveryCode.Text = code ?? Loc.Chrome("settings.recovery.unavailable");
            recoveryCode.IsVisible = true;
        };
        content.Children.Add(show);
        content.Children.Add(recoveryCode);
    }

    // ── Lock / Remove ────────────────────────────────────────────────────────

    // Lock the library — drop its key on this device so it reopens to the unlock
    // prompt while local files stay — and remove it from this device — delete its
    // local data directory and return to the welcome chooser, leaving any cloud copy
    // untouched. Both drive the coordinator's window swap. Remove is a two-step armed
    // confirm: the first click reads the outbox snapshot to decide whether to call
    // out unlanded cloud work and arms; the second forgets and swaps.
    private void BuildLibraryLifecycle(StackPanel content, List<Action<Settings>> renderers)
    {
        // Captured from the live settings so the handlers act on the current library.
        var libraryId = string.Empty;
        var hasCloudHome = false;

        var lockButton = new Button { Content = Loc.Chrome("settings.lock_library") };
        lockButton.Click += async (_, _) =>
        {
            var (current, error) = _app.Sync.LockActiveLibrary();
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                return;
            }
            // The key is forgotten now, so re-opening lands on the unlock prompt.
            _window?.Close();
            await _switchLibrary(libraryId);
        };
        content.Children.Add(lockButton);

        content.Children.Add(SectionLabel(Loc.Chrome("settings.remove.title")));
        var removeFooter = SecondaryLabel(string.Empty);
        content.Children.Add(removeFooter);
        var removeButton = new Button { Content = Loc.Chrome("settings.remove.button") };
        content.Children.Add(removeButton);
        var removeConfirm = DialogUi.Danger();
        content.Children.Add(removeConfirm);

        var armed = false;
        removeButton.Click += async (_, _) =>
        {
            if (!armed)
            {
                var (snapshotCurrent, snapshotResult) = await _app.Sync.OutboxSnapshot();
                if (!snapshotCurrent)
                {
                    return;
                }
                if (snapshotResult.Error is not null)
                {
                    // The outbox read only informs the confirmation copy; a failure
                    // here can't call out pending cloud work but doesn't block arming.
                    BaeDiagnostics.Logger.Warning(
                        $"Could not read the outbox snapshot for the remove confirmation: {snapshotResult.Error}");
                }
                var hasPendingCloudWork = snapshotResult.Snapshot is { } snapshot
                    && ForgetLibraryModel.HasPendingCloudWork(snapshot.UploadGroups.Length, snapshot.PendingDeletes);
                removeConfirm.Text = string.Join(
                    " ",
                    ForgetLibraryModel.ConfirmKeys(hasCloudHome, hasPendingCloudWork).Select(Loc.Chrome));
                removeConfirm.IsVisible = true;
                armed = true;
                return;
            }

            var (forgetCurrent, error) = await _app.Sync.ForgetLibrary();
            if (!forgetCurrent)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(Loc.Chrome("settings.remove.failed", "error", error));
                return;
            }
            // The local directory is gone; tear the handle down and return to welcome.
            _window?.Close();
            await _closeToWelcome();
        };

        renderers.Add(fresh =>
        {
            libraryId = fresh.LibraryId;
            hasCloudHome = fresh.HasCloudHome;
            removeFooter.Text = Loc.Chrome(ForgetLibraryModel.FooterKey(fresh.HasCloudHome));
        });
    }

    // ── Shared chrome / helpers ──────────────────────────────────────────────

    private void ShowSettingsError(string message)
    {
        _settingsError.Text = message;
        _settingsError.IsVisible = true;
    }

    private void ClearSettingsError()
    {
        _settingsError.Text = string.Empty;
        _settingsError.IsVisible = false;
    }

    internal static TextBlock SectionLabel(string text)
    {
        var t = new TextBlock { Text = text, FontWeight = FontWeight.SemiBold, Margin = new Thickness(0, 8, 0, 0) };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        return t;
    }

    internal static TextBlock SecondaryLabel(string text)
    {
        var t = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return t;
    }

    // A labeled text field: a caption over the box.
    internal static Control LabeledField(string label, TextBox box)
    {
        var caption = new TextBlock { Text = label, FontSize = 12.5 };
        caption[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return new StackPanel { Spacing = 4, Children = { caption, box } };
    }

    internal static StackPanel ButtonRow(params Control[] buttons)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (var button in buttons)
        {
            row.Children.Add(button);
        }
        return row;
    }
}
