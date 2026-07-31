using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Shared dialog building blocks used across the library-lifecycle, import,
// album-detail, and storage screens.
internal static class DialogPrimitives
{
    // The folder picker for a make-local (unmanage) transition, run in the app
    // window. Returns the chosen path, or null when the user cancelled. Shared by
    // the storage dialog and the album-detail storage band so both take the same
    // destination for the transfer.
    internal static async System.Threading.Tasks.Task<string?> PickUnmanageFolder(IntPtr windowHandle)
    {
        var picker = new global::Windows.Storage.Pickers.FolderPicker();
        picker.FileTypeFilter.Add("*");
        WinRT.Interop.InitializeWithWindow.Initialize(picker, windowHandle);
        var folder = await picker.PickSingleFolderAsync();
        return folder?.Path;
    }

    // The user-facing label for a storage transition, matching the macOS
    // "Storage…" sheet / context menu wording. Shared by the storage row menu and
    // the album-detail storage band.
    internal static string StorageActionLabel(BridgeReleaseStorageAction action) => action switch
    {
        BridgeReleaseStorageAction.MakeRemote => Loc.Chrome("storage.action.manage"),
        BridgeReleaseStorageAction.MakeLocal => Loc.Chrome("storage.action.unmanage"),
        BridgeReleaseStorageAction.Pin => Loc.Chrome("storage.action.pin"),
        BridgeReleaseStorageAction.Unpin => Loc.Chrome("storage.action.unpin"),
        _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
    };

    // A release's resting storage state, mirroring macOS's precedence: unmanaged
    // first, then pinned (kept offline), then managed (cloud). Shared by the
    // storage row's Storage cell and the album-detail band's status line.
    internal static string RestingStorageLabel(bool isManaged, bool pinned)
    {
        if (!isManaged)
        {
            return Loc.Chrome("storage.state.unmanaged");
        }
        return pinned ? Loc.Chrome("storage.pinned") : Loc.Chrome("storage.state.managed");
    }

    // A dismiss-only error dialog: a title, an optional detail body, and a single
    // "OK" button that closes it.
    internal static async System.Threading.Tasks.Task ShowError(XamlRoot? xamlRoot, string title, string? message = null)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            CloseButtonText = Loc.Chrome("action.ok"),
            XamlRoot = xamlRoot,
        };
        if (message is not null)
        {
            dialog.Content = message;
        }

        await dialog.ShowAsync();
    }

    // A cover-art thumbnail tile: an image over a one-line caption, the whole tile
    // a borderless button. The caller wires Click — the change-cover gallery
    // applies the selection immediately, the import-confirm gallery carries it and
    // highlights the picked tile via the button's border.
    // The caller supplies the image control so the image store can bind to it
    // and apply the decode when it lands.
    internal static Button CoverTile(Image thumb, string caption)
    {
        thumb.Stretch = Stretch.UniformToFill;
        thumb.Width = 120;
        thumb.Height = 120;
        var label = new TextBlock
        {
            Text = caption,
            TextTrimming = TextTrimming.CharacterEllipsis,
            MaxWidth = 120,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        var stack = new StackPanel { Spacing = 4 };
        stack.Children.Add(thumb);
        stack.Children.Add(label);
        return new Button
        {
            Content = stack,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4),
        };
    }

    // A code-display block: the QR image (when it renders), the code as selectable
    // monospaced text, and a Copy button. Shared by the join screen (this device's
    // code), the approve flow (the invite code), and the recovery reveal.
    internal static StackPanel BuildCodeDisplay(string code)
    {
        var panel = new StackPanel { Spacing = 8, HorizontalAlignment = HorizontalAlignment.Center };

        var qr = QrCode.Image(code);
        if (qr is not null)
        {
            panel.Children.Add(new Image
            {
                Source = qr,
                Width = 180,
                Height = 180,
                Stretch = Stretch.Uniform,
            });
        }

        panel.Children.Add(new TextBox
        {
            Text = code,
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("Consolas"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        });

        var copy = new Button
        {
            Content = Loc.Chrome("action.copy"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        copy.Click += (_, _) => ClipboardHelper.CopyToClipboard(code);
        panel.Children.Add(copy);

        return panel;
    }
}
