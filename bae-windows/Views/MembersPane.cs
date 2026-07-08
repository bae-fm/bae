using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The devices (membership) list in settings: one row per device with its role and
// a "this device" marker, plus — for an owner — an add-device button and a
// two-step remove on each other device. Removing rotates the library key, so it
// confirms inline (a nested ContentDialog can't open over the settings dialog).
internal sealed class MembersPane
{
    private readonly SessionStore _session;

    public MembersPane(SessionStore session)
    {
        _session = session;
    }

    // Load the library's devices into a host panel: one row per device (short
    // fingerprint + role + "this device" marker), and — for an owner — an
    // "Add a device…" button plus a Remove control on each other device. Runs the
    // blocking generated bridge off the UI thread. <paramref name="onAddDevice"/> arms the
    // approve flow (which the caller runs once the settings dialog closes).
    public async System.Threading.Tasks.Task LoadInto(StackPanel host, Action onAddDevice)
    {
        var (current, result) = await _session.RunForCurrentHandle(NativeBae.GetMembers);
        if (!current)
        {
            return;
        }
        host.Children.Clear();

        if (result.Error is not null)
        {
            host.Children.Add(new TextBlock
            {
                Text = result.Error,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }

        var membership = result.Membership;
        if (membership is null)
        {
            host.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.load_failed"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }

        foreach (var member in membership.Members)
        {
            host.Children.Add(BuildMemberRow(member, host, onAddDevice));
        }

        if (membership.SelfIsOwner)
        {
            var add = new Button { Content = Loc.Chrome("members.add") };
            add.Click += (_, _) => onAddDevice();
            host.Children.Add(add);
        }
    }

    // One device row: fingerprint + role badge + "this device" marker, plus a
    // two-step Remove for the owner on every other device. Removing rotates the
    // library key, so it confirms inline (a second click) — a nested ContentDialog
    // can't open over the settings dialog.
    private FrameworkElement BuildMemberRow(BridgeMember member, StackPanel host, Action onAddDevice)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };

        var labels = new StackPanel { Spacing = 0 };
        labels.Children.Add(new TextBlock
        {
            Text = member.Fingerprint,
            FontFamily = new FontFamily("Consolas"),
        });
        if (member.IsSelf)
        {
            labels.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.this_device"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });
        }
        row.Children.Add(labels);

        row.Children.Add(new TextBlock
        {
            Text = MemberFormat.RoleLabel(member.Role),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            VerticalAlignment = VerticalAlignment.Center,
        });

        // The owner can remove any device but its own.
        if (member.CanRemove)
        {
            var remove = new Button { Content = Loc.Chrome("members.remove") };
            var status = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };
            var armed = false;
            remove.Click += async (_, _) =>
            {
                if (!armed)
                {
                    status.Text = Loc.Chrome("members.remove_confirm");
                    status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                    status.Visibility = Visibility.Visible;
                    armed = true;
                    return;
                }

                remove.IsEnabled = false;
                var pubkey = member.Pubkey;
                var (current, error) = await _session.RunForCurrentHandle(
                    handle => NativeBae.RemoveMember(handle, pubkey));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    status.Text = error;
                    status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                    status.Visibility = Visibility.Visible;
                    remove.IsEnabled = true;
                    armed = false;
                    return;
                }

                // Reload the list in place so the removed device disappears.
                await LoadInto(host, onAddDevice);
            };
            row.Children.Add(remove);

            var rowWithStatus = new StackPanel { Spacing = 4 };
            rowWithStatus.Children.Add(row);
            rowWithStatus.Children.Add(status);
            return rowWithStatus;
        }

        return row;
    }
}
