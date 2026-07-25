using System;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The devices (membership) list in settings: one row per device with its role and
// a "this device" marker, plus — for an owner — an add-device button and a
// two-step remove on each other device. Removing rotates the library key, so it
// confirms inline. Reads and writes go through the sync service, never NativeBae.
internal sealed class MembersPane
{
    private readonly AppService _app;

    public MembersPane(AppService app)
    {
        _app = app;
    }

    // Load the library's devices into a host panel: one row per device (short
    // fingerprint + role + "this device" marker), and — for an owner — an
    // "Add a device…" button plus a Remove control on each other device. onAddDevice
    // arms the approve flow.
    public async Task LoadInto(StackPanel host, Action onAddDevice)
    {
        var (current, result) = await _app.Sync.GetMembers();
        if (!current)
        {
            return;
        }
        host.Children.Clear();

        if (result.Error is not null)
        {
            host.Children.Add(ErrorLine(result.Error));
            return;
        }

        var membership = result.Membership;
        if (membership is null)
        {
            host.Children.Add(ErrorLine(Loc.Chrome("members.load_failed")));
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
    // library key, so it confirms inline (a second click).
    private Control BuildMemberRow(BridgeMember member, StackPanel host, Action onAddDevice)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };

        var labels = new StackPanel { Spacing = 0 };
        labels.Children.Add(new TextBlock { Text = member.Fingerprint, FontFamily = new FontFamily("monospace") });
        if (member.IsSelf)
        {
            var self = new TextBlock { Text = Loc.Chrome("members.this_device") };
            self[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            labels.Children.Add(self);
        }
        row.Children.Add(labels);

        var role = new TextBlock
        {
            Text = MemberFormat.RoleLabel(member.Role),
            VerticalAlignment = VerticalAlignment.Center,
        };
        role[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        row.Children.Add(role);

        // The owner can remove any device but its own.
        if (!member.CanRemove)
        {
            return row;
        }

        var remove = new Button { Content = Loc.Chrome("members.remove") };
        var status = new TextBlock { TextWrapping = TextWrapping.Wrap, IsVisible = false };
        var armed = false;
        remove.Click += async (_, _) =>
        {
            if (!armed)
            {
                status.Text = Loc.Chrome("members.remove_confirm");
                status[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
                status.IsVisible = true;
                armed = true;
                return;
            }

            remove.IsEnabled = false;
            var (current, error) = await _app.Sync.RemoveMember(member.Pubkey);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                status.Text = error;
                status[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");
                status.IsVisible = true;
                remove.IsEnabled = true;
                armed = false;
                return;
            }

            // Reload the list in place so the removed device disappears.
            await LoadInto(host, onAddDevice);
        };
        row.Children.Add(remove);

        return new StackPanel { Spacing = 4, Children = { row, status } };
    }

    private static TextBlock ErrorLine(string text)
    {
        var t = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");
        return t;
    }
}
