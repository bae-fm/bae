using System;
using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The library manager: switch between the libraries on this device, rename one,
// or add a new one. Restore-from-code lives only in the first-run flow. Once a
// library is open the first-run flow never shows, so this is the only way to
// reach other libraries or create another.
internal sealed class LibrariesDialog
{
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly SessionStore _session;
    private readonly Func<List<BridgeLibrary>> _loadLibraries;
    private readonly Func<Action<string>, string?> _createLibrary;
    private readonly Func<string, System.Threading.Tasks.Task> _switchLibrary;

    public LibrariesDialog(
        Func<XamlRoot?> xamlRoot,
        SessionStore session,
        Func<List<BridgeLibrary>> loadLibraries,
        Func<Action<string>, string?> createLibrary,
        Func<string, System.Threading.Tasks.Task> switchLibrary)
    {
        _xamlRoot = xamlRoot;
        _session = session;
        _loadLibraries = loadLibraries;
        _createLibrary = createLibrary;
        _switchLibrary = switchLibrary;
    }

    public async System.Threading.Tasks.Task Show()
    {
        var libraries = _loadLibraries();

        var list = new StackPanel { Spacing = 4, MinWidth = 360 };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        list.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("libraries.title"),
            Content = new ScrollViewer { Content = list, MaxHeight = 420 },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };

        foreach (var library in libraries)
        {
            var id = library.Id;
            var isActive = library.IsActive;

            var row = new Grid { ColumnSpacing = 4 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            // Click the name to switch to that library; the active one can't switch
            // to itself but can still be renamed.
            var switchButton = new Button
            {
                Content = isActive
                    ? Loc.Chrome("libraries.active", "name", library.Name)
                    : library.Name,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                IsEnabled = !isActive,
            };
            switchButton.Click += async (_, _) =>
            {
                dialog.Hide();
                await _switchLibrary(id);
            };
            Grid.SetColumn(switchButton, 0);
            row.Children.Add(switchButton);

            // Rename via a flyout editor (a nested ContentDialog can't open over this
            // one). Saving updates the row label in place; no list rebuild needed.
            var nameBox = new TextBox { Text = library.Name, MinWidth = 220 };
            var renameStatus = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };
            var saveName = new Button { Content = Loc.Chrome("action.save") };
            var renameContent = new StackPanel { Spacing = 6 };
            renameContent.Children.Add(nameBox);
            renameContent.Children.Add(saveName);
            renameContent.Children.Add(renameStatus);
            var renameFlyout = new Flyout { Content = renameContent };
            saveName.Click += (_, _) =>
            {
                var newName = nameBox.Text?.Trim() ?? string.Empty;
                if (string.IsNullOrEmpty(newName))
                {
                    return;
                }

                var (current, error) = _session.WithCurrentHandle(
                    handle => NativeBae.RenameLibrary(handle, id, newName));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    renameStatus.Text = error;
                    renameStatus.Visibility = Visibility.Visible;
                    return;
                }

                switchButton.Content = isActive
                    ? Loc.Chrome("libraries.active", "name", newName)
                    : newName;
                // Keep the editor's value in sync so reopening it shows the saved
                // name, not the stale snapshot it was seeded with.
                nameBox.Text = newName;
                renameFlyout.Hide();
            };
            var renameButton = new Button { Content = Loc.Chrome("libraries.rename"), Flyout = renameFlyout };
            Grid.SetColumn(renameButton, 1);
            row.Children.Add(renameButton);

            list.Children.Add(row);
        }

        var newButton = new Button
        {
            Content = Loc.Chrome("libraries.new"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        newButton.Click += async (_, _) =>
        {
            var newId = _createLibrary(message =>
            {
                status.Text = message;
                status.Visibility = Visibility.Visible;
            });
            if (newId is null)
            {
                return;
            }

            dialog.Hide();
            await _switchLibrary(newId);
        };
        list.Children.Add(newButton);

        await dialog.ShowAsync();
    }
}
