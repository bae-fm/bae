using System;
using System.IO;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Platform.Storage;

namespace Bae.Desktop;

// The library manager: switch between the libraries on this device, rename one, or
// add a new one. Restore-from-code lives only in the first-run flow; once a library
// is open the first-run flow never shows, so this is the only way to reach other
// libraries or create another. Presented in the window's modal host; switching or
// creating closes it and hands the id to the coordinator, which swaps the window.
internal sealed class LibrariesDialog
{
    private readonly AppService _app;
    private readonly ModalHost _host;
    private readonly Func<string, Task> _switchLibrary;

    public LibrariesDialog(AppService app, ModalHost host, Func<string, Task> switchLibrary)
    {
        _app = app;
        _host = host;
        _switchLibrary = switchLibrary;
    }

    public Task Show() => _host.Show(Build);

    private Control Build(Action close)
    {
        var status = DialogUi.Danger();

        // Neutral confirmation for "copy ID": persists until overwritten or the
        // dialog closes.
        var copyFeedback = new TextBlock { TextWrapping = TextWrapping.Wrap, IsVisible = false };
        copyFeedback[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        var list = new StackPanel { Spacing = 4, MinWidth = 480 };
        list.Children.Add(DialogUi.Title(Loc.Chrome("libraries.title")));
        list.Children.Add(status);
        list.Children.Add(copyFeedback);

        foreach (var library in LibraryDiscovery.Load(message =>
        {
            status.Text = message;
            status.IsVisible = true;
        }))
        {
            list.Children.Add(BuildRow(library, status, copyFeedback, close));
        }

        var newButton = new Button
        {
            Content = Loc.Chrome("libraries.new"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        newButton.Click += async (_, _) =>
        {
            var newId = LibraryDiscovery.Create(message =>
            {
                status.Text = message;
                status.IsVisible = true;
            });
            if (newId is null)
            {
                return;
            }

            close();
            await _switchLibrary(newId);
        };
        list.Children.Add(newButton);

        return new ScrollViewer { Content = list, MaxHeight = 420 };
    }

    private Control BuildRow(
        uniffi.bae_bridge.BridgeLibrary library, TextBlock status, TextBlock copyFeedback, Action close)
    {
        var id = library.Id;
        var isActive = library.IsActive;
        var path = library.Path;

        var row = new Grid { ColumnSpacing = 4, ColumnDefinitions = new ColumnDefinitions("*,Auto,Auto,Auto") };

        // Click the name to switch to that library; the active one can't switch to
        // itself but can still be renamed. A library whose config will not load is
        // listed with the reason and cannot be switched to — visible rather than
        // silently absent.
        var broken = library.Error;
        var switchButton = new Button
        {
            Content = broken is not null
                ? $"{library.Name} — {broken}"
                : isActive
                    ? Loc.Chrome("libraries.active", "name", library.Name)
                    : library.Name,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            IsEnabled = !isActive && broken is null,
        };
        switchButton.Click += async (_, _) =>
        {
            close();
            await _switchLibrary(id);
        };
        Grid.SetColumn(switchButton, 0);
        row.Children.Add(switchButton);

        // Open the library's folder in the OS file manager. Success needs no
        // feedback; failure lands on the shared danger line.
        var revealButton = new Button { Content = Loc.Chrome("libraries.reveal") };
        revealButton.Click += async (_, _) =>
        {
            var launcher = TopLevel.GetTopLevel(revealButton)?.Launcher;
            var revealed = false;
            if (launcher is not null)
            {
                try
                {
                    revealed = await launcher.LaunchDirectoryInfoAsync(new DirectoryInfo(path));
                }
                catch (Exception exception)
                {
                    BaeDiagnostics.Logger.Warning($"Reveal library folder failed: {exception.Message}");
                }
            }
            if (!revealed)
            {
                status.Text = Loc.Chrome("libraries.reveal_failed");
                status.IsVisible = true;
            }
        };
        Grid.SetColumn(revealButton, 1);
        row.Children.Add(revealButton);

        // Copy the library's id to the clipboard, confirming in the neutral line.
        var copyIdButton = new Button { Content = Loc.Chrome("libraries.copy_id") };
        copyIdButton.Click += (_, _) =>
        {
            ClipboardHelper.CopyToClipboard(copyIdButton, id);
            copyFeedback.Text = Loc.Chrome("libraries.id_copied");
            copyFeedback.IsVisible = true;
        };
        Grid.SetColumn(copyIdButton, 2);
        row.Children.Add(copyIdButton);

        // Rename via a flyout editor. Saving updates the row label in place; no list
        // rebuild needed.
        var nameBox = new TextBox { Text = library.Name, MinWidth = 220 };
        var renameStatus = DialogUi.Danger();
        var saveName = new Button { Content = Loc.Chrome("action.save") };
        var renameFlyout = new Flyout
        {
            Content = new StackPanel { Spacing = 6, Children = { nameBox, saveName, renameStatus } },
        };
        saveName.Click += (_, _) =>
        {
            var newName = nameBox.Text?.Trim() ?? string.Empty;
            if (string.IsNullOrEmpty(newName))
            {
                return;
            }

            var (current, error) = _app.Sync.RenameLibrary(id, newName);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                renameStatus.Text = error;
                renameStatus.IsVisible = true;
                return;
            }

            switchButton.Content = isActive
                ? Loc.Chrome("libraries.active", "name", newName)
                : newName;
            // Keep the editor value in sync so reopening it shows the saved name.
            nameBox.Text = newName;
            renameFlyout.Hide();
        };
        var renameButton = new Button { Content = Loc.Chrome("libraries.rename"), Flyout = renameFlyout };
        Grid.SetColumn(renameButton, 3);
        row.Children.Add(renameButton);

        return row;
    }
}
