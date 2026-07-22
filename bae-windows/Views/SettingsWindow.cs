using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.System;
// `Windows.System` (needed for VirtualKey) also declares a DispatcherQueue;
// alias the WinUI one so the unqualified name stays unambiguous (CS0104).
using DispatcherQueue = Microsoft.UI.Dispatching.DispatcherQueue;

namespace Bae.Windows;

// The settings window: Discogs key, cloud sync (disconnect / S3 / OAuth),
// export destination/pattern/presets, MCP automation, devices, recovery code,
// updates, lock, and remove. Reads the current settings through the settings
// store and re-renders when a config invalidation (or an in-window
// connect/disconnect) reloads them; those registrations live only while the
// window is open. Being a window rather than a ContentDialog, dialogs (the
// preset editor, confirmations, the approve flow) open over it directly — the
// old close-and-run-after dances are gone.
internal sealed class SettingsWindow
{
    private readonly SessionStore _session;
    private readonly DispatcherQueue _dispatcher;
    private readonly SettingsStore _settings;
    private readonly MembersPane _membersPane;
    private readonly ApproveDeviceDialog _approveDialog;
    private readonly UpdateService _updateService;
    private readonly ProjectionRegistry _projections;
    private readonly Action<string> _openLibrary;
    private readonly Func<System.Threading.Tasks.Task> _closeToWelcome;

    // The one open settings window; Show re-activates it instead of stacking
    // a second.
    private Window? _window;

    public SettingsWindow(
        SessionStore session,
        DispatcherQueue dispatcher,
        SettingsStore settings,
        MembersPane membersPane,
        ApproveDeviceDialog approveDialog,
        UpdateService updateService,
        ProjectionRegistry projections,
        Action<string> openLibrary,
        Func<System.Threading.Tasks.Task> closeToWelcome)
    {
        _session = session;
        _dispatcher = dispatcher;
        _settings = settings;
        _membersPane = membersPane;
        _approveDialog = approveDialog;
        _updateService = updateService;
        _projections = projections;
        _openLibrary = openLibrary;
        _closeToWelcome = closeToWelcome;
    }

    public void Show()
    {
        if (_window is { } open)
        {
            open.Activate();
            return;
        }
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }

        var (current, s) = _session.WithCurrentHandle(NativeBae.GetSettings);
        if (!current)
        {
            return;
        }

        // Host-originated telemetry: the settings screen opened, through the
        // standalone sink. Infallible.
        NativeBae.ReportScreen(BaeDiagnostics.Handle, BridgeScreen.Settings);

        // Discogs key state machine. The token input is the only local draft state;
        // the configured/valid state comes from generated bridge settings, re-read on
        // a config invalidation. not_configured/rejected → editable input + Save; valid →
        // "connected" + Remove; unvalidated → that label + Re-check + Remove. Save
        // and Re-check validate over the network, so they run off the UI thread and
        // show "Validating…" while in flight.
        //
        // Two text lines: `status` is the persisted state (driven only by
        // RenderDiscogs from the settings re-read, plus the in-flight "Validating…");
        // `settingsErrorText` is local feedback for an action — a rejected key,
        // a settings write failure, a re-check / remove failure — cleared when
        // the next action starts. Keeping them apart means an unrelated
        // a config-invalidation re-render can't wipe the rejection note.
        var status = new TextBlock { TextWrapping = TextWrapping.Wrap, Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray) };
        var settingsErrorText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            Visibility = Visibility.Collapsed,
        };
        var tokenBox = new TextBox { PlaceholderText = Loc.Chrome("settings.discogs.token_placeholder") };
        var save = new Button { Content = Loc.Chrome("settings.discogs.save") };
        var recheck = new Button { Content = Loc.Chrome("settings.discogs.recheck") };
        var remove = new Button { Content = Loc.Chrome("settings.discogs.remove") };
        var discogsBusy = false;

        void ShowSettingsError(string message)
        {
            settingsErrorText.Text = message;
            settingsErrorText.Visibility = Visibility.Visible;
        }

        void ClearSettingsError()
        {
            settingsErrorText.Text = string.Empty;
            settingsErrorText.Visibility = Visibility.Collapsed;
        }

        // Drive the controls from the persisted status: which buttons show, whether
        // the input is editable, and the status line. Called on open and on every
        // config-invalidation re-read. The draft text and the local error line are left
        // alone — they belong to the user's in-progress input, not the stored state.
        void RenderDiscogs(Settings settings)
        {
            if (discogsBusy)
            {
                return;
            }

            tokenBox.Visibility = settings.DiscogsConfigured ? Visibility.Collapsed : Visibility.Visible;
            save.Visibility = settings.DiscogsConfigured ? Visibility.Collapsed : Visibility.Visible;
            remove.Visibility = settings.DiscogsConfigured ? Visibility.Visible : Visibility.Collapsed;
            recheck.Visibility = settings.DiscogsNeedsRecheck ? Visibility.Visible : Visibility.Collapsed;
            status.Text = settings.DiscogsConfigured ? settings.DiscogsStatusText : string.Empty;
        }

        save.Click += async (_, _) =>
        {
            var token = tokenBox.Text ?? string.Empty;
            if (string.IsNullOrEmpty(token) || discogsBusy)
            {
                return;
            }

            discogsBusy = true;
            ClearSettingsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var (current, outcome) = await _session.RunForCurrentHandle(
                handle => NativeBae.SaveDiscogsToken(handle, token));
            discogsBusy = false;
            if (!current)
            {
                return;
            }
            switch (outcome)
            {
                case "valid":
                case "unvalidated":
                    // Stored: a config-invalidation re-read settles the controls and label.
                    status.Text = string.Empty;
                    break;
                case "rejected":
                    // Nothing stored, so no config invalidation fires — keep the draft and
                    // surface the rejection.
                    status.Text = string.Empty;
                    ShowSettingsError(Loc.Chrome("settings.discogs.rejected"));
                    break;
                default:
                    status.Text = string.Empty;
                    ShowSettingsError(Loc.Chrome("settings.discogs.save_failed"));
                    break;
            }
        };
        recheck.Click += async (_, _) =>
        {
            if (discogsBusy)
            {
                return;
            }

            discogsBusy = true;
            ClearSettingsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var (current, error) = await _session.RunForCurrentHandle(NativeBae.RevalidateDiscogsToken);
            discogsBusy = false;
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
            // On success a config-invalidation re-read settles the controls and label.
        };
        remove.Click += async (_, _) =>
        {
            if (discogsBusy)
            {
                return;
            }

            ClearSettingsError();
            // Removing clears the config flag, firing a config invalidation — the re-read
            // restores the editable input. Nothing is patched inline here.
            var (current, error) = await _session.RunForCurrentHandle(NativeBae.DeleteDiscogsToken);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
        };

        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        buttons.Children.Add(save);
        buttons.Children.Add(recheck);
        buttons.Children.Add(remove);

        var syncStatus = new TextBlock { Text = s.SyncStatusText };
        // Two-step disconnect: the first click surfaces the data-loss warning (when
        // releases live only in the cloud) inline and arms; the second confirms.
        var disconnect = new Button { Content = Loc.Chrome("settings.sync.disconnect") };
        var disconnectArmed = false;
        disconnect.Click += async (_, _) =>
        {
            if (!disconnectArmed)
            {
                var (warningCurrent, countResult) = await _session.RunForCurrentHandle(NativeBae.CloudOnlyReleaseCount);
                if (!warningCurrent)
                {
                    return;
                }
                var (count, countError) = countResult;
                string? warning = countError;
                if (warning is null && count > 0)
                {
                    warning = Loc.Core("core.sync.cloud_only_releases", "count", count.Value);
                }
                if (warning is not null)
                {
                    syncStatus.Text = Loc.Chrome("settings.sync.disconnect_confirm", "warning", warning);
                    disconnectArmed = true;
                    return;
                }
            }

            disconnectArmed = false;
            var (disconnectCurrent, error) = _session.WithCurrentHandle(NativeBae.DisconnectCloud);
            if (!disconnectCurrent)
            {
                return;
            }
            if (error is not null)
            {
                syncStatus.Text = error;
            }
            else
            {
                _settings.Reload();
            }
        };
        var syncNow = new Button { Content = Loc.Chrome("settings.sync.now") };
        syncNow.Click += (_, _) => _session.WithCurrentHandle(NativeBae.TriggerSync);
        var syncButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        syncButtons.Children.Add(disconnect);
        syncButtons.Children.Add(syncNow);

        // Opaque (encrypted) vs browsable (stored in the clear), applied to
        // whichever provider is connected below. Defaults to the secure choice.
        // Not access control — the bucket's own credentials gate it either way.
        var storagePicker = new ComboBox { Header = Loc.Chrome("settings.storage.mode"), SelectedIndex = 0 };
        storagePicker.Items.Add(new ComboBoxItem { Content = Loc.Chrome("settings.storage.opaque"), Tag = "opaque" });
        storagePicker.Items.Add(new ComboBoxItem { Content = Loc.Chrome("settings.storage.browsable"), Tag = "browsable" });
        string SelectedStorage() =>
            (storagePicker.SelectedItem as ComboBoxItem)?.Tag as string ?? "opaque";

        // OAuth providers: signing in runs the browser flow in the core, so it
        // blocks until the user finishes — run it off the UI thread.
        Button CloudButton(string label, string provider)
        {
            var button = new Button { Content = label };
            button.Click += async (_, _) =>
            {
                if (!OAuthCreds.Available)
                {
                    syncStatus.Text = OAuthCreds.RegistrationError
                        ?? Loc.Chrome("cloud.signin.not_configured");
                    return;
                }
                syncStatus.Text = Loc.Chrome("cloud.signin.in_progress", "provider", label);
                var storage = SelectedStorage();
                var (current, error) = await _session.RunForCurrentHandle(
                    handle => NativeBae.SignInCloud(handle, provider, storage));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    syncStatus.Text = error;
                }
                else
                {
                    _settings.Reload();
                }
            };
            return button;
        }

        // Only offer the OAuth providers this build's native library supports.
        // An S3-only build returns just S3, so no sign-in button renders.
        var available = NativeBae.AvailableCloudProviders();
        var oauthButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (var wire in new[] { "google_drive", "dropbox", "onedrive" })
        {
            if (available.Contains(wire))
            {
                oauthButtons.Children.Add(CloudButton(BridgeDisplay.ProviderDisplayName(wire), wire));
            }
        }

        var content = new StackPanel { Spacing = 8, MinWidth = 360 };
        var libraryLabel = new TextBlock { Text = Loc.Chrome("settings.library_label", "name", s.LibraryName) };
        var pauseBetweenSides = new CheckBox
        {
            Content = Loc.Chrome("settings.playback.pause_between_sides"),
            IsChecked = s.PauseBetweenSides,
        };
        var refreshingSettings = false;
        async System.Threading.Tasks.Task SetPauseBetweenSides(bool enabled)
        {
            if (refreshingSettings)
            {
                return;
            }

            ClearSettingsError();
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.SetPauseBetweenSides(handle, enabled));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                refreshingSettings = true;
                pauseBetweenSides.IsChecked = !enabled;
                refreshingSettings = false;
            }
        }

        pauseBetweenSides.Checked += async (_, _) => await SetPauseBetweenSides(true);
        pauseBetweenSides.Unchecked += async (_, _) => await SetPauseBetweenSides(false);

        // Whether quitting performs the graceful shutdown that saves the
        // current track, position, queue, and volume for the next launch to
        // restore. Device-local (no bridge round-trip), so no error snap-back
        // and no Refresh() involvement — a config invalidation doesn't carry it.
        var restoreOnLaunch = new CheckBox
        {
            Content = Loc.Chrome("settings.playback.restore_on_launch"),
            IsChecked = PersistPlaybackStore.Load(),
        };
        restoreOnLaunch.Checked += (_, _) => PersistPlaybackStore.Save(true);
        restoreOnLaunch.Unchecked += (_, _) => PersistPlaybackStore.Save(false);
        var restoreOnLaunchHelp = new TextBlock
        {
            Text = Loc.Chrome("settings.playback.restore_on_launch_help"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };

        // Export: filename pattern, default formats, and presets. The section
        // renders from the settings re-read and writes through the bridge;
        // errors land in the shared settings error line.
        var exportSection = new ExportSettingsSection(
            _session,
            _settings,
            () => _window?.Content.XamlRoot,
            ShowSettingsError,
            ClearSettingsError);

        var automationLabel = new TextBlock
        {
            Text = Loc.Chrome("settings.automation.label"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        var mcpEnabled = new CheckBox
        {
            Content = Loc.Chrome("settings.automation.enable_mcp"),
            IsChecked = s.McpEnabled,
        };
        var mcpPort = new TextBox
        {
            Header = Loc.Chrome("settings.automation.port"),
            Text = s.McpPort.ToString(CultureInfo.InvariantCulture),
        };
        var mcpStatus = new TextBlock
        {
            Text = s.McpStatusText,
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };
        var saveMcp = new Button { Content = Loc.Chrome("action.save") };
        var refreshMcp = new Button { Content = Loc.Chrome("settings.automation.refresh") };
        var copyMcpToken = new Button { Content = Loc.Chrome("settings.automation.copy_token") };
        var rotateMcpToken = new Button { Content = Loc.Chrome("settings.automation.rotate_token") };
        var mcpButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        mcpButtons.Children.Add(saveMcp);
        mcpButtons.Children.Add(refreshMcp);
        mcpButtons.Children.Add(copyMcpToken);
        mcpButtons.Children.Add(rotateMcpToken);

        void RenderMcp(Settings settings)
        {
            if (refreshingSettings)
            {
                return;
            }

            refreshingSettings = true;
            mcpEnabled.IsChecked = settings.McpEnabled;
            mcpPort.Text = settings.McpPort.ToString(CultureInfo.InvariantCulture);
            mcpStatus.Text = settings.McpStatusText;
            refreshingSettings = false;
        }

        async System.Threading.Tasks.Task SetMcpConfig(bool enabled)
        {
            if (refreshingSettings)
            {
                return;
            }

            if (!ushort.TryParse(mcpPort.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var port) || port == 0)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.invalid_port"));
                return;
            }

            ClearSettingsError();
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.SetMcpServerConfig(handle, enabled, port));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                refreshingSettings = true;
                mcpEnabled.IsChecked = !enabled;
                refreshingSettings = false;
                return;
            }
            _settings.Reload();
        }

        async System.Threading.Tasks.Task RefreshMcpStatus()
        {
            var (current, status) = await _session.RunForCurrentHandle(NativeBae.McpServerStatus);
            if (!current)
            {
                return;
            }
            mcpStatus.Text = Settings.McpStatusTextFor(status);
        }

        async System.Threading.Tasks.Task CopyMcpToken(Func<AppHandle, string?> readToken, string successKey)
        {
            ClearSettingsError();
            var (current, token) = await _session.RunForCurrentHandle(readToken);
            if (!current)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            ClipboardHelper.CopyToClipboard(token);
            mcpStatus.Text = Loc.Chrome(successKey);
        }

        mcpEnabled.Checked += async (_, _) => await SetMcpConfig(true);
        mcpEnabled.Unchecked += async (_, _) => await SetMcpConfig(false);
        saveMcp.Click += async (_, _) => await SetMcpConfig(mcpEnabled.IsChecked == true);
        refreshMcp.Click += async (_, _) => await RefreshMcpStatus();
        copyMcpToken.Click += async (_, _) =>
            await CopyMcpToken(NativeBae.GetMcpToken, "settings.automation.token_copied");
        rotateMcpToken.Click += async (_, _) =>
        {
            ClearSettingsError();
            var (tokenCurrent, token) = await _session.RunForCurrentHandle(NativeBae.GenerateMcpToken);
            if (!tokenCurrent)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.SetMcpToken(handle, token));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                return;
            }
            ClipboardHelper.CopyToClipboard(token);
            mcpStatus.Text = Loc.Chrome("settings.automation.token_rotated");
        };

        var discogsLabel = new TextBlock { Text = Loc.Chrome("settings.discogs.label") };
        content.Children.Add(libraryLabel);
        content.Children.Add(pauseBetweenSides);
        content.Children.Add(restoreOnLaunch);
        content.Children.Add(restoreOnLaunchHelp);
        content.Children.Add(exportSection.View);
        content.Children.Add(automationLabel);
        content.Children.Add(mcpEnabled);
        content.Children.Add(mcpPort);
        content.Children.Add(mcpButtons);
        content.Children.Add(mcpStatus);
        content.Children.Add(discogsLabel);
        content.Children.Add(tokenBox);
        content.Children.Add(buttons);
        content.Children.Add(status);
        content.Children.Add(settingsErrorText);
        RenderDiscogs(s);
        exportSection.Render(s);
        // S3-compatible provider form. The core probes the bucket before saving.
        var s3Bucket = new TextBox { Header = Loc.Chrome("s3.field.bucket") };
        var s3Region = new TextBox { Header = Loc.Chrome("s3.field.region") };
        var s3Endpoint = new TextBox { Header = Loc.Chrome("s3.field.endpoint") };
        var s3KeyPrefix = new TextBox { Header = Loc.Chrome("s3.field.key_prefix") };
        var s3AccessKey = new TextBox { Header = Loc.Chrome("s3.field.access_key") };
        var s3SecretKey = new PasswordBox { Header = Loc.Chrome("s3.field.secret_key") };
        var connectS3 = new Button { Content = Loc.Chrome("settings.s3.connect") };
        connectS3.Click += async (_, _) =>
        {
            syncStatus.Text = Loc.Chrome("settings.s3.connecting");
            var storage = SelectedStorage();
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.SaveSyncConfig(
                    handle,
                    s3Bucket.Text ?? string.Empty,
                    s3Region.Text ?? string.Empty,
                    s3Endpoint.Text ?? string.Empty,
                    s3KeyPrefix.Text ?? string.Empty,
                    s3AccessKey.Text ?? string.Empty,
                    s3SecretKey.Password ?? string.Empty,
                    storage));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                syncStatus.Text = error;
            }
            else
            {
                _settings.Reload();
            }
        };
        var s3Form = new StackPanel { Spacing = 6 };
        s3Form.Children.Add(s3Bucket);
        s3Form.Children.Add(s3Region);
        s3Form.Children.Add(s3Endpoint);
        s3Form.Children.Add(s3KeyPrefix);
        s3Form.Children.Add(s3AccessKey);
        s3Form.Children.Add(s3SecretKey);
        s3Form.Children.Add(connectS3);

        content.Children.Add(syncStatus);
        content.Children.Add(syncButtons);
        content.Children.Add(storagePicker);
        content.Children.Add(oauthButtons);
        content.Children.Add(s3Form);

        // Devices (membership): list the library's devices with their role and a
        // "this device" marker. The owner can add a device (which opens the
        // approve flow) or remove one (which rotates the library key). The list
        // loads off the UI thread; the add-device button only renders for an owner.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("members.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var membersHost = new StackPanel { Spacing = 8 };
        membersHost.Children.Add(new ProgressRing { IsActive = true, Width = 20, Height = 20 });
        content.Children.Add(membersHost);

        // Recovery: the restore code is now a recovery secret only — it restores
        // this library on a new device when there's no other device available to
        // approve it. Anyone with it has full access, so it's revealed on demand,
        // behind a warning, never shown by default.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.recovery.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.recovery.intro"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        var recoveryCode = new TextBox
        {
            Header = Loc.Chrome("settings.recovery.label"),
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("Consolas"),
            Visibility = Visibility.Collapsed,
        };
        var showRecoveryCode = new Button { Content = Loc.Chrome("settings.recovery.show") };
        showRecoveryCode.Click += async (_, _) =>
        {
            var (current, code) = await _session.RunForCurrentHandle(NativeBae.GenerateRestoreCode);
            if (!current)
            {
                return;
            }
            recoveryCode.Text = code ?? Loc.Chrome("settings.recovery.unavailable");
            recoveryCode.Visibility = Visibility.Visible;
        };
        content.Children.Add(showRecoveryCode);
        content.Children.Add(recoveryCode);

        // Updates: the installed version, and — for a Velopack install — a manual
        // check that downloads in the background and applies on restart. A dev run
        // or a loose-zip copy is not an install, so only the version line shows.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.updates.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var installedVersion = _updateService.InstalledVersion is { } version
            ? UpdateFlowDisplay.VersionDisplay(version)
            : AppMetadata.ConfiguredString("BaeGitCommit");
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.updates.version", "version", installedVersion),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });

        Button? updateRestartButton = null;
        Action? unsubscribeUpdates = null;
        if (_updateService.IsAvailable)
        {
            var updateStatus = new TextBlock
            {
                TextWrapping = TextWrapping.Wrap,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            };
            var checkUpdates = new Button { Content = Loc.Chrome("settings.updates.check") };
            var restartUpdate = new Button { Content = Loc.Chrome("settings.updates.restart") };
            updateRestartButton = restartUpdate;

            void RenderUpdates(UpdateFlowState state)
            {
                if (UpdateFlowDisplay.StatusFor(state) is { } mapped)
                {
                    updateStatus.Text = mapped.Args is { } args
                        ? Loc.Chrome(mapped.Key, args)
                        : Loc.Chrome(mapped.Key);
                    updateStatus.Visibility = Visibility.Visible;
                }
                else
                {
                    updateStatus.Text = string.Empty;
                    updateStatus.Visibility = Visibility.Collapsed;
                }
                checkUpdates.IsEnabled = UpdateFlowDisplay.CheckEnabled(state);
                restartUpdate.Visibility = UpdateFlowDisplay.RestartVisible(state)
                    ? Visibility.Visible
                    : Visibility.Collapsed;
            }

            checkUpdates.Click += async (_, _) => await _updateService.CheckAsync();

            // State transitions arrive on a worker thread; marshal to the UI
            // thread. Subscribed for the window's lifetime, so reopening
            // settings reflects a background download that finished while it
            // was closed (the phase lives on the service, not the window).
            void OnUpdateStateChanged(UpdateFlowState state) =>
                _dispatcher.TryEnqueue(() => RenderUpdates(state));
            _updateService.StateChanged += OnUpdateStateChanged;
            unsubscribeUpdates = () => _updateService.StateChanged -= OnUpdateStateChanged;

            RenderUpdates(_updateService.State);
            content.Children.Add(updateStatus);
            content.Children.Add(checkUpdates);
            content.Children.Add(restartUpdate);
        }

        // Lock this library: forget its encryption key on this device. Sync stops
        // and the library reopens to the unlock prompt; local files stay.
        var lockButton = new Button { Content = Loc.Chrome("settings.lock_library") };
        content.Children.Add(lockButton);

        // Remove this library from this device: delete its local data directory,
        // clear the active-library pointer, and drop its encryption key, leaving
        // any cloud copy untouched. Two-step armed confirm, like disconnect: the
        // first click reads the outbox snapshot off the UI thread to decide
        // whether to call out unlanded cloud work, renders the confirmation body
        // inline, and arms; the second click forgets the library and closes the
        // window.
        var removeArmed = false;
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.remove.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var removeFooter = new TextBlock
        {
            Text = Loc.Chrome(ForgetLibraryModel.FooterKey(s.HasCloudHome)),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };
        content.Children.Add(removeFooter);
        var removeButton = new Button { Content = Loc.Chrome("settings.remove.button") };
        content.Children.Add(removeButton);
        var removeConfirmText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(removeConfirmText);

        var window = new Window { Title = Loc.Chrome("settings.title") };
        window.Content = new ScrollViewer
        {
            Content = content,
            Padding = new Thickness(24, 16, 24, 24),
        };
        _window = window;

        lockButton.Click += async (_, _) =>
        {
            var (lockCurrent, error) = await _session.RunForCurrentHandle(NativeBae.LockActiveLibrary);
            if (!lockCurrent)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                return;
            }

            // The key is forgotten now, so re-opening lands on the unlock prompt.
            window.Close();
            await _session.ShutdownAndFreeCurrentHandle();
            _openLibrary(s.LibraryId);
        };
        removeButton.Click += async (_, _) =>
        {
            if (!removeArmed)
            {
                var (snapshotCurrent, snapshotResult) = await _session.RunForCurrentHandle(NativeBae.OutboxSnapshot);
                if (!snapshotCurrent)
                {
                    return;
                }
                if (snapshotResult.Error is not null)
                {
                    // The outbox read only informs the confirmation copy; a
                    // failure here doesn't block the confirm from arming, it
                    // just can't call out pending cloud work.
                    BaeDiagnostics.Logger.Warning(
                        $"Could not read the outbox snapshot for the remove confirmation: {snapshotResult.Error}");
                }
                var hasPendingCloudWork = snapshotResult.Snapshot is { } snapshot
                    && ForgetLibraryModel.HasPendingCloudWork(snapshot.UploadGroups.Length, snapshot.PendingDeletes);
                var hasCloudHome = _settings.Current?.HasCloudHome ?? s.HasCloudHome;
                removeConfirmText.Text = string.Join(
                    " ",
                    ForgetLibraryModel.ConfirmKeys(hasCloudHome, hasPendingCloudWork).Select(key => Loc.Chrome(key)));
                removeConfirmText.Visibility = Visibility.Visible;
                removeArmed = true;
                return;
            }

            var (forgetCurrent, error) = await _session.RunForCurrentHandle(NativeBae.ForgetLibrary);
            if (!forgetCurrent)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(Loc.Chrome("settings.remove.failed", "error", error));
                return;
            }

            // The local directory is gone; tear the handle down and return to
            // the welcome chooser, mirroring macOS's closeLibrary().
            window.Close();
            await _closeToWelcome();
        };

        // The restart button applies a staged update. ApplyUpdatesAndRestart
        // exits the process, so this is a second app-exit path: flush telemetry
        // through the standalone sink first, then gate the state-saving
        // shutdown on the same restore-on-launch preference OnClosed does.
        // Only wired when the updates section rendered it.
        if (updateRestartButton is not null)
        {
            updateRestartButton.Click += async (_, _) =>
            {
                window.Close();
                BaeDiagnostics.Flush();
                if (PersistPlaybackStore.Load())
                {
                    await _session.ShutdownAndFreeCurrentHandle();
                }
                _updateService.ApplyAndRestart();
            };
        }

        // Load the device list into its placeholder. The add-device button
        // (owner-only) runs the approve flow's dialog over the main window —
        // the settings window stays open — and refreshes the device list when
        // the flow finishes.
        async void OnAddDevice()
        {
            await _approveDialog.Show();
            _ = _membersPane.LoadInto(membersHost, OnAddDevice);
        }
        _ = _membersPane.LoadInto(membersHost, OnAddDevice);

        // Re-read the (generated bridge-pre-computed) settings into the live labels so a
        // config invalidation — or a connect/disconnect in this window — updates
        // them in place instead of requiring a reopen. The store's Reload raises
        // Changed; this renders from its fresh snapshot.
        void Refresh()
        {
            if (_settings.Current is not { } fresh)
            {
                return;
            }

            syncStatus.Text = fresh.SyncStatusText;
            libraryLabel.Text = Loc.Chrome("settings.library_label", "name", fresh.LibraryName);
            refreshingSettings = true;
            pauseBetweenSides.IsChecked = fresh.PauseBetweenSides;
            refreshingSettings = false;
            exportSection.Render(fresh);
            RenderMcp(fresh);
            RenderDiscogs(fresh);
            removeFooter.Text = Loc.Chrome(ForgetLibraryModel.FooterKey(fresh.HasCloudHome));
        }
        _settings.Changed += Refresh;
        // A config invalidation reloads the store while the window is open;
        // the registration is disposed on close.
        var configRegistration = _projections.Register(
            typeof(BridgeInvalidation.Config), () => _settings.Reload());

        // A key saved while offline lands "unvalidated"; opening settings is a
        // chance to settle it now that there may be connectivity. The core no-ops
        // unless the stored key is actually unvalidated, so call unconditionally;
        // on a result it changes the status, firing a config invalidation.
        _ = _session.RunForCurrentHandle(NativeBae.RevalidateDiscogsToken);

        // Closed fires for the user's close chrome and for every flow above
        // that calls window.Close().
        window.Closed += (_, _) =>
        {
            _settings.Changed -= Refresh;
            configRegistration.Dispose();
            unsubscribeUpdates?.Invoke();
            _window = null;
        };

        window.Activate();
        // Size the client area; AppWindow speaks physical pixels, so scale by
        // the live rasterization factor.
        var scale = window.Content.XamlRoot?.RasterizationScale ?? 1.0;
        window.AppWindow.ResizeClient(new global::Windows.Graphics.SizeInt32(
            (int)(560 * scale), (int)(760 * scale)));
    }
}
