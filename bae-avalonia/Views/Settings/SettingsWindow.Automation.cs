using System;
using System.Collections.Generic;
using System.Globalization;
using Avalonia.Controls;
using Avalonia.Layout;

namespace Bae.Desktop;

// The settings window's Automation (MCP) section: enable toggle, port, the
// save/refresh/copy-token/rotate-token actions, and the server status line. Config
// writes round-trip through the config re-read (no optimistic mutation); token
// values are returned only for the copy/rotate actions and never held in state.
internal sealed partial class SettingsWindow
{
    private void BuildAutomation(StackPanel content, List<Action<Settings>> renderers)
    {
        var enabled = new CheckBox { Content = Loc.Chrome("settings.automation.enable_mcp") };
        var portBox = new TextBox();
        var status = SecondaryLabel(string.Empty);
        var save = new Button { Content = Loc.Chrome("action.save") };
        var refresh = new Button { Content = Loc.Chrome("settings.automation.refresh") };
        var copyToken = new Button { Content = Loc.Chrome("settings.automation.copy_token") };
        var rotateToken = new Button { Content = Loc.Chrome("settings.automation.rotate_token") };

        async System.Threading.Tasks.Task SetConfig(bool enable)
        {
            if (_refreshingSettings)
            {
                return;
            }
            if (!ushort.TryParse(portBox.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var port) || port == 0)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.invalid_port"));
                return;
            }
            ClearSettingsError();
            var (current, error) = await _app.Automation.SetServerConfig(enable, port);
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
        save.Click += async (_, _) => await SetConfig(enabled.IsChecked == true);
        refresh.Click += async (_, _) =>
        {
            var (current, serverStatus) = await _app.Automation.ServerStatus();
            if (!current)
            {
                return;
            }
            status.Text = Settings.McpStatusTextFor(serverStatus);
        };
        copyToken.Click += async (_, _) =>
        {
            ClearSettingsError();
            var (current, token) = await _app.Automation.GetToken();
            if (!current)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            ClipboardHelper.CopyToClipboard(copyToken, token);
            status.Text = Loc.Chrome("settings.automation.token_copied");
        };
        rotateToken.Click += async (_, _) =>
        {
            ClearSettingsError();
            var (tokenCurrent, token) = await _app.Automation.GenerateToken();
            if (!tokenCurrent)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            var (current, error) = await _app.Automation.SetToken(token);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                return;
            }
            ClipboardHelper.CopyToClipboard(rotateToken, token);
            status.Text = Loc.Chrome("settings.automation.token_rotated");
        };

        content.Children.Add(SectionLabel(Loc.Chrome("settings.automation.label")));
        content.Children.Add(enabled);
        content.Children.Add(LabeledField(Loc.Chrome("settings.automation.port"), portBox));
        content.Children.Add(ButtonRow(save, refresh, copyToken, rotateToken));
        content.Children.Add(status);

        renderers.Add(fresh =>
        {
            _refreshingSettings = true;
            enabled.IsChecked = fresh.McpEnabled;
            portBox.Text = fresh.McpPort.ToString(CultureInfo.InvariantCulture);
            status.Text = fresh.McpStatusText;
            _refreshingSettings = false;
        });
    }
}
