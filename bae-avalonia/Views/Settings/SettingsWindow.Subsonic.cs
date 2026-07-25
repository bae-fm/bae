using System;
using System.Collections.Generic;
using System.Globalization;
using Avalonia.Controls;

namespace Bae.Desktop;

// The settings window's Subsonic server section: enable toggle, port, username, an
// allow-network toggle, a keyring-backed password, and the server status line. The
// password is write-only from here (committed to the keyring) and never read back
// into state; the config writes round-trip through the config re-read.
internal sealed partial class SettingsWindow
{
    private void BuildSubsonic(StackPanel content, List<Action<Settings>> renderers)
    {
        var enabled = new CheckBox { Content = Loc.Chrome("settings.subsonic.enable") };
        var portBox = new TextBox();
        var usernameBox = new TextBox();
        var allowNetwork = new CheckBox { Content = Loc.Chrome("settings.subsonic.allow_network") };
        var passwordBox = new TextBox { PasswordChar = '•' };
        var status = SecondaryLabel(string.Empty);
        var save = new Button { Content = Loc.Chrome("action.save") };
        var refresh = new Button { Content = Loc.Chrome("settings.subsonic.refresh") };
        var savePassword = new Button { Content = Loc.Chrome("settings.subsonic.save_password") };

        async System.Threading.Tasks.Task SetConfig(bool enable)
        {
            if (_refreshingSettings)
            {
                return;
            }
            if (!ushort.TryParse(portBox.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var port) || port == 0)
            {
                ShowSettingsError(Loc.Chrome("settings.subsonic.invalid_port"));
                return;
            }
            var bindAddress = allowNetwork.IsChecked == true ? "0.0.0.0" : "127.0.0.1";
            ClearSettingsError();
            var (current, error) = await _app.Subsonic.SetServerConfig(
                enable, port, usernameBox.Text ?? string.Empty, bindAddress);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _refreshingSettings = true;
                enabled.IsChecked = !enable;
                _refreshingSettings = false;
                return;
            }
            _app.SettingsStore.Reload();
        }

        enabled.IsCheckedChanged += async (_, _) => await SetConfig(enabled.IsChecked == true);
        allowNetwork.IsCheckedChanged += async (_, _) => await SetConfig(enabled.IsChecked == true);
        save.Click += async (_, _) => await SetConfig(enabled.IsChecked == true);
        refresh.Click += async (_, _) =>
        {
            var (current, serverStatus) = await _app.Subsonic.ServerStatus();
            if (!current)
            {
                return;
            }
            status.Text = Settings.SubsonicStatusTextFor(serverStatus);
        };
        savePassword.Click += async (_, _) =>
        {
            ClearSettingsError();
            var (current, error) = await _app.Subsonic.SetPassword(passwordBox.Text ?? string.Empty);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                return;
            }
            status.Text = Loc.Chrome("settings.subsonic.password_saved");
        };

        content.Children.Add(SectionLabel(Loc.Chrome("settings.subsonic.label")));
        content.Children.Add(enabled);
        content.Children.Add(LabeledField(Loc.Chrome("settings.subsonic.port"), portBox));
        content.Children.Add(LabeledField(Loc.Chrome("settings.subsonic.username"), usernameBox));
        content.Children.Add(allowNetwork);
        content.Children.Add(LabeledField(Loc.Chrome("settings.subsonic.password"), passwordBox));
        content.Children.Add(SecondaryLabel(Loc.Chrome("settings.subsonic.password_help")));
        content.Children.Add(ButtonRow(save, refresh, savePassword));
        content.Children.Add(status);

        renderers.Add(fresh =>
        {
            _refreshingSettings = true;
            enabled.IsChecked = fresh.SubsonicEnabled;
            portBox.Text = fresh.SubsonicPort.ToString(CultureInfo.InvariantCulture);
            usernameBox.Text = fresh.SubsonicUsername;
            allowNetwork.IsChecked = fresh.SubsonicBindAddress != "127.0.0.1";
            status.Text = fresh.SubsonicStatusText;
            _refreshingSettings = false;
        });
    }
}
