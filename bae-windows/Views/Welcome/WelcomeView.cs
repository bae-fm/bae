using System;
using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The welcome chooser, shown before any library is open: on first run (no library
// on disk) and after closing one. Shows the bae wordmark and subtitle over three
// stacked actions — create new library (prominent, the default action), join a
// library, restore from cloud — plus the libraries already on disk to reopen,
// when any exist. Creating writes the new library's keys (Windows Credential
// Manager) and on-disk layout, then hands the id to the coordinator, which
// closes this window and opens the library window.
internal sealed class WelcomeView
{
    // The shared width of the stacked action buttons, matching the macOS
    // welcome column's fixed button width.
    private const int ActionWidth = 240;

    private readonly Panel _host;
    private readonly Action<string> _setStatus;
    private readonly Func<List<BridgeLibrary>> _loadLibraries;
    private readonly Func<Action<string>, string?> _createLibrary;
    private readonly Action<string> _openLibrary;
    private readonly Func<System.Threading.Tasks.Task> _showJoinLibrary;
    private readonly Func<System.Threading.Tasks.Task> _showRestoreFromCloud;

    // The welcome chooser controls (the on-disk library list plus the three
    // actions), shown when no library is open; removed once one is opened.
    private StackPanel? _welcome;

    public WelcomeView(
        Panel host,
        Action<string> setStatus,
        Func<List<BridgeLibrary>> loadLibraries,
        Func<Action<string>, string?> createLibrary,
        Action<string> openLibrary,
        Func<System.Threading.Tasks.Task> showJoinLibrary,
        Func<System.Threading.Tasks.Task> showRestoreFromCloud)
    {
        _host = host;
        _setStatus = setStatus;
        _loadLibraries = loadLibraries;
        _createLibrary = createLibrary;
        _openLibrary = openLibrary;
        _showJoinLibrary = showJoinLibrary;
        _showRestoreFromCloud = showRestoreFromCloud;
    }

    public void Show()
    {
        // Re-entrant safety: drop any welcome panel from a previous showing so we
        // don't stack two.
        Dismiss();

        var libraries = _loadLibraries();
        // The wordmark and subtitle carry the empty state; the status line only
        // speaks when there are libraries to choose between (or an error).
        _setStatus(libraries.Count > 0 ? Loc.Chrome("welcome.choose_library") : string.Empty);

        _welcome = new StackPanel { Spacing = 24, HorizontalAlignment = HorizontalAlignment.Center };

        _welcome.Children.Add(new TextBlock
        {
            Text = "bae",
            FontSize = 48,
            FontWeight = Microsoft.UI.Text.FontWeights.Bold,
            HorizontalAlignment = HorizontalAlignment.Center,
        });
        _welcome.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("welcome.subtitle"),
            FontSize = 16,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            HorizontalAlignment = HorizontalAlignment.Center,
        });

        if (libraries.Count > 0)
        {
            var librariesSection = new StackPanel { Spacing = 8 };
            librariesSection.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("welcome.your_libraries"),
                HorizontalAlignment = HorizontalAlignment.Center,
            });
            foreach (var library in libraries)
            {
                var id = library.Id;
                var openButton = new Button
                {
                    Content = library.Name,
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                };
                openButton.Click += (_, _) => _openLibrary(id);
                librariesSection.Children.Add(openButton);
            }
            _welcome.Children.Add(librariesSection);
        }

        var createButton = new Button
        {
            Content = Loc.Chrome("welcome.create_library"),
            Width = ActionWidth,
            HorizontalAlignment = HorizontalAlignment.Center,
            Style = (Style)Application.Current.Resources["AccentButtonStyle"],
        };
        var joinButton = new Button
        {
            Content = Loc.Chrome("welcome.join_library"),
            Width = ActionWidth,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        var restoreCloudButton = new Button
        {
            Content = Loc.Chrome("restore.from_cloud"),
            Width = ActionWidth,
            HorizontalAlignment = HorizontalAlignment.Center,
        };

        // Failure surface for create: an inline line under the actions, red, per
        // the app's error text convention.
        var createError = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            MaxWidth = 400,
            HorizontalAlignment = HorizontalAlignment.Center,
            Visibility = Visibility.Collapsed,
        };

        createButton.Click += async (_, _) =>
        {
            var label = createButton.Content;
            createButton.Content = new ProgressRing { IsActive = true, Width = 16, Height = 16 };
            createButton.IsEnabled = joinButton.IsEnabled = restoreCloudButton.IsEnabled = false;
            createError.Visibility = Visibility.Collapsed;

            // The create callback reports its failure message from the worker
            // thread; it lands in the inline error line below, not the status bar.
            string? failure = null;
            var libraryId = await System.Threading.Tasks.Task.Run(
                () => _createLibrary(message => failure = message));

            if (libraryId is null)
            {
                createButton.Content = label;
                createButton.IsEnabled = joinButton.IsEnabled = restoreCloudButton.IsEnabled = true;
                createError.Text = failure ?? Loc.Chrome("library.create_failed");
                createError.Visibility = Visibility.Visible;
                return;
            }

            _openLibrary(libraryId);
        };
        joinButton.Click += async (_, _) => await _showJoinLibrary();
        restoreCloudButton.Click += async (_, _) => await _showRestoreFromCloud();

        var actions = new StackPanel { Spacing = 12 };
        actions.Children.Add(createButton);
        actions.Children.Add(joinButton);
        actions.Children.Add(restoreCloudButton);
        actions.Children.Add(createError);
        _welcome.Children.Add(actions);
        _host.Children.Add(_welcome);

        // Create is the default action: focused on show, so Enter triggers it —
        // the WinUI equivalent of the macOS default button.
        createButton.Loaded += (_, _) => createButton.Focus(FocusState.Programmatic);
    }

    public void Dismiss()
    {
        if (_welcome is not null)
        {
            _host.Children.Remove(_welcome);
            _welcome = null;
        }
    }
}
