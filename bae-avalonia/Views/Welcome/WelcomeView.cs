using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The welcome chooser, shown before any library is open: on first run (no library
// on disk) and after closing one. Shows the bae wordmark and subtitle over three
// stacked actions — create new library (prominent, the default action), join a
// library, restore from cloud — plus the libraries already on disk to reopen,
// when any exist. Creating writes the new library's keys and on-disk layout, then
// hands the id to the coordinator, which closes this window and opens the library
// window. Every color reads a theme resource, so the chooser renders correctly in
// either OS appearance.
internal sealed class WelcomeView : UserControl
{
    // The shared width of the stacked action buttons, matching the macOS welcome
    // column's fixed button width.
    private const int ActionWidth = 240;

    private readonly Action<string> _setStatus;
    private readonly Func<List<BridgeLibrary>> _loadLibraries;
    private readonly Func<Action<string>, string?> _createLibrary;
    private readonly Action<string> _openLibrary;
    private readonly Func<Task> _showJoinLibrary;
    private readonly Func<Task> _showRestoreFromCloud;

    public WelcomeView(
        Action<string> setStatus,
        Func<List<BridgeLibrary>> loadLibraries,
        Func<Action<string>, string?> createLibrary,
        Action<string> openLibrary,
        Func<Task> showJoinLibrary,
        Func<Task> showRestoreFromCloud)
    {
        _setStatus = setStatus;
        _loadLibraries = loadLibraries;
        _createLibrary = createLibrary;
        _openLibrary = openLibrary;
        _showJoinLibrary = showJoinLibrary;
        _showRestoreFromCloud = showRestoreFromCloud;

        HorizontalAlignment = HorizontalAlignment.Stretch;
        VerticalAlignment = VerticalAlignment.Stretch;
        Content = Build();
    }

    private Control Build()
    {
        var libraries = _loadLibraries();
        // The wordmark and subtitle carry the empty state; the status line only
        // speaks when there are libraries to choose between (or an error).
        _setStatus(libraries.Count > 0 ? Loc.Chrome("welcome.choose_library") : string.Empty);

        var column = new StackPanel
        {
            Spacing = 24,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };

        column.Children.Add(ThemedText("bae", 48, FontWeight.Bold, "BaeTextPrimaryBrush"));
        column.Children.Add(ThemedText(
            Loc.Chrome("welcome.subtitle"), 16, FontWeight.Normal, "BaeTextSecondaryBrush"));

        if (libraries.Count > 0)
        {
            var librariesSection = new StackPanel { Spacing = 8 };
            librariesSection.Children.Add(ThemedText(
                Loc.Chrome("welcome.your_libraries"), 14, FontWeight.Normal, "BaeTextSecondaryBrush"));
            foreach (var library in libraries)
            {
                var id = library.Id;
                var openButton = new Button
                {
                    Content = library.Name,
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                    HorizontalContentAlignment = HorizontalAlignment.Center,
                };
                openButton.Click += (_, _) => _openLibrary(id);
                librariesSection.Children.Add(openButton);
            }
            column.Children.Add(librariesSection);
        }

        var createButton = new Button
        {
            Content = Loc.Chrome("welcome.create_library"),
            Width = ActionWidth,
            HorizontalAlignment = HorizontalAlignment.Center,
            HorizontalContentAlignment = HorizontalAlignment.Center,
            IsDefault = true,
        };
        // The prominent action: FluentTheme's accent class paints it with the
        // terracotta SystemAccentColor the theme dictionaries set.
        createButton.Classes.Add("accent");

        var joinButton = new Button
        {
            Content = Loc.Chrome("welcome.join_library"),
            Width = ActionWidth,
            HorizontalAlignment = HorizontalAlignment.Center,
            HorizontalContentAlignment = HorizontalAlignment.Center,
        };
        var restoreCloudButton = new Button
        {
            Content = Loc.Chrome("restore.from_cloud"),
            Width = ActionWidth,
            HorizontalAlignment = HorizontalAlignment.Center,
            HorizontalContentAlignment = HorizontalAlignment.Center,
        };

        // Failure surface for create: an inline line under the actions, red, per
        // the app's error text convention.
        var createError = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            MaxWidth = 400,
            HorizontalAlignment = HorizontalAlignment.Center,
            TextAlignment = TextAlignment.Center,
            IsVisible = false,
        };
        createError[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");

        createButton.Click += async (_, _) =>
        {
            var label = createButton.Content;
            createButton.Content = new Spinner { Width = 16, Height = 16 };
            createButton.IsEnabled = joinButton.IsEnabled = restoreCloudButton.IsEnabled = false;
            createError.IsVisible = false;

            // The create callback reports its failure message from the worker
            // thread; it lands in the inline error line below, not the status bar.
            string? failure = null;
            var libraryId = await Task.Run(() => _createLibrary(message => failure = message));

            if (libraryId is null)
            {
                createButton.Content = label;
                createButton.IsEnabled = joinButton.IsEnabled = restoreCloudButton.IsEnabled = true;
                createError.Text = failure ?? Loc.Chrome("library.create_failed");
                createError.IsVisible = true;
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
        column.Children.Add(actions);

        return column;
    }

    // A TextBlock whose foreground reads a theme brush by key, so it re-resolves
    // against the current OS appearance rather than carrying a literal color.
    private static TextBlock ThemedText(string text, double size, FontWeight weight, string fgKey)
    {
        var tb = new TextBlock
        {
            Text = text,
            FontSize = size,
            FontWeight = weight,
            HorizontalAlignment = HorizontalAlignment.Center,
            TextAlignment = TextAlignment.Center,
        };
        tb[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(fgKey);
        return tb;
    }
}
