using System;
using System.Collections.Generic;
using Avalonia.Controls;

namespace Bae.Desktop;

// The settings window's Casting section: one toggle for the whole feature. Core
// is what the toggle actually gates — while off it browses no network and starts
// no session — so this section only writes the setting, and warns first when the
// write would cut a live session short. Mirrors macOS's CastingSettingsTab.
internal sealed partial class SettingsWindow
{
    private void BuildCast(StackPanel content, List<Action<Settings>> renderers)
    {
        var enabled = new CheckBox { Content = Loc.Chrome("settings.casting.enable") };

        void Write(bool enable)
        {
            ClearSettingsError();
            var error = _app.CastStore.SetEnabled(enable);
            if (error is not null)
            {
                ShowSettingsError(error);
            }
            // Either way the config re-read is what settles the box: a refused
            // write leaves it where it was, with nothing to undo here.
            _app.SettingsStore.Reload();
        }

        enabled.IsCheckedChanged += async (_, _) =>
        {
            if (_refreshingSettings)
            {
                return;
            }
            var enable = enabled.IsChecked == true;
            var device = _app.CastStore.CastingDeviceName;
            if (!CastSettingsModel.NeedsDisconnectConfirmation(enable, device))
            {
                Write(enable);
                return;
            }
            // Put the box back while the question is up, so the setting only
            // moves once the answer is in.
            _refreshingSettings = true;
            enabled.IsChecked = true;
            _refreshingSettings = false;
            await _modalHost.Show(close => BuildCastDisconnectConfirm(device!, close, () => Write(false)));
        };

        content.Children.Add(SectionLabel(Loc.Chrome("settings.casting.label")));
        content.Children.Add(enabled);
        content.Children.Add(SecondaryLabel(Loc.Chrome("settings.casting.footer")));

        renderers.Add(fresh =>
        {
            _refreshingSettings = true;
            enabled.IsChecked = fresh.CastEnabled;
            _refreshingSettings = false;
        });
    }

    // "Turning casting off will stop the session on <device>" — confirm or back out.
    private static Control BuildCastDisconnectConfirm(string device, Action close, Action confirm)
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("settings.casting.confirm_title")));
        column.Children.Add(DialogUi.Body(Loc.Chrome("settings.casting.confirm_body", "device", device)));

        var cancel = new Button { Content = Loc.Chrome("action.cancel") };
        cancel.Click += (_, _) => close();
        var turnOff = DialogUi.Primary(Loc.Chrome("settings.casting.confirm_turn_off"));
        turnOff.Click += (_, _) =>
        {
            close();
            confirm();
        };
        column.Children.Add(DialogUi.Actions(cancel, turnOff));
        return column;
    }
}
