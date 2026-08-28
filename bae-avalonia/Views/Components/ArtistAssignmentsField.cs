using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>A compact ordered artist-assignment field. Existing artists retain
/// their library identity; typed names remain explicit new-artist seeds.</summary>
internal sealed class ArtistAssignmentsField : UserControl
{
    private readonly LibraryService _library;
    private readonly Action<IReadOnlyList<BridgeArtistAssignment>> _onChange;
    private readonly Action? _onUseAlbumArtists;
    private readonly Button _field;
    private readonly StackPanel _tokens = new() { Spacing = 5 };
    private readonly StackPanel _results = new() { Spacing = 3 };
    private readonly TextBox _query = new();
    private readonly TextBlock _error = DialogUi.Danger();
    private readonly Spinner _spinner = new() { Width = 14, Height = 14, IsVisible = false };
    private List<BridgeArtistAssignment> _assignments;
    private bool _inheritsAlbumArtists;

    internal ArtistAssignmentsField(
        IReadOnlyList<BridgeArtistAssignment> assignments,
        LibraryService library,
        Action<IReadOnlyList<BridgeArtistAssignment>> onChange,
        bool inheritsAlbumArtists = false,
        Action? onUseAlbumArtists = null)
    {
        _assignments = assignments.ToList();
        _library = library;
        _onChange = onChange;
        _inheritsAlbumArtists = inheritsAlbumArtists;
        _onUseAlbumArtists = onUseAlbumArtists;

        _field = new Button
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            Padding = new Avalonia.Thickness(8, 5),
        };
        _field.Click += (_, _) => OpenEditor();
        RenderField();
        Content = _field;
    }

    internal IReadOnlyList<BridgeArtistAssignment> Assignments => _assignments;
    internal bool InheritsAlbumArtists => _inheritsAlbumArtists;

    internal void SetAssignments(
        IReadOnlyList<BridgeArtistAssignment> assignments,
        bool inheritsAlbumArtists = false)
    {
        _assignments = assignments.ToList();
        _inheritsAlbumArtists = inheritsAlbumArtists;
        RenderTokens();
        RenderField();
    }

    private void RenderField()
    {
        Control value;
        if (_inheritsAlbumArtists || _assignments.Count == 0)
        {
            var text = new TextBlock
            {
                Text = _inheritsAlbumArtists
                ? Loc.Chrome("artist.assignments.album_artist")
                : string.Empty,
                TextTrimming = TextTrimming.CharacterEllipsis,
                MaxLines = 1,
                VerticalAlignment = VerticalAlignment.Center,
            };
            text[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension("BaeTextSecondaryBrush");
            value = text;
        }
        else
        {
            value = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                Spacing = 8,
                ClipToBounds = true,
            };
            var row = (StackPanel)value;
            foreach (var assignment in _assignments)
            {
                row.Children.Add(ArtistAssignmentDisplay.Label(assignment));
            }
        }
        var arrow = new TextBlock
        {
            Text = "⌄",
            FontSize = 11,
            VerticalAlignment = VerticalAlignment.Center,
        };
        arrow[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        _field.Content = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
            Children =
            {
                value,
                arrow.WithGridColumn(1),
            },
        };
    }

    private void OpenEditor()
    {
        RenderTokens();
        _results.Children.Clear();
        _error.IsVisible = false;
        _query.Text = string.Empty;

        var search = new Button { Content = Loc.Chrome("action.search") };
        search.Click += async (_, _) => await Search();
        var add = new Button { Content = Loc.Chrome("artist.assignments.add") };
        add.Click += (_, _) => AddTyped();
        _query.KeyDown += async (_, args) =>
        {
            if (args.Key == Avalonia.Input.Key.Enter)
            {
                await Search();
            }
        };

        var searchRow = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto,Auto,Auto"),
            ColumnSpacing = 6,
            Children =
            {
                _query,
                search.WithGridColumn(1),
                add.WithGridColumn(2),
                _spinner.WithGridColumn(3),
            },
        };
        var editor = new StackPanel
        {
            Width = 330,
            Spacing = 8,
            Margin = new Avalonia.Thickness(10),
            Children = { _tokens, searchRow, _error, _results },
        };
        _field.Flyout = new Flyout { Content = editor };
        _field.Flyout.ShowAt(_field);
    }

    private void RenderTokens()
    {
        _tokens.Children.Clear();
        if (_onUseAlbumArtists is not null && !_inheritsAlbumArtists)
        {
            var inherit = new Button
            {
                Content = Loc.Chrome("artist.assignments.album_artist"),
            };
            inherit.Click += (_, _) =>
            {
                _inheritsAlbumArtists = true;
                _assignments.Clear();
                _onUseAlbumArtists();
                RenderTokens();
                RenderField();
            };
            _tokens.Children.Add(inherit);
        }
        for (var index = 0; index < _assignments.Count; index++)
        {
            var assignmentIndex = index;
            var remove = new Button
            {
                Content = "×",
                Padding = new Avalonia.Thickness(5, 0),
            };
            Avalonia.Automation.AutomationProperties.SetName(
                remove,
                Loc.Chrome("artist.assignments.remove"));
            remove.Click += (_, _) =>
            {
                var next = _assignments.ToList();
                next.RemoveAt(assignmentIndex);
                SetExplicit(next);
            };
            _tokens.Children.Add(new Grid
            {
                ColumnDefinitions = new ColumnDefinitions("*,Auto"),
                Children =
                {
                    ArtistAssignmentDisplay.Label(_assignments[index]),
                    remove.WithGridColumn(1),
                },
            });
        }
    }

    private async System.Threading.Tasks.Task Search()
    {
        var query = (_query.Text ?? string.Empty).Trim();
        _results.Children.Clear();
        _error.IsVisible = false;
        if (query.Length == 0)
        {
            return;
        }
        _spinner.IsVisible = true;
        var (current, result) = await _library.SearchArtists(query);
        _spinner.IsVisible = false;
        if (!current)
        {
            return;
        }
        if (result.Error is { } error)
        {
            _error.Text = error;
            _error.IsVisible = true;
            return;
        }
        if (result.Artists is null)
        {
            _error.Text = Loc.Chrome("import.failed");
            _error.IsVisible = true;
            return;
        }
        foreach (var match in result.Artists)
        {
            var button = new Button
            {
                HorizontalAlignment = HorizontalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Left,
                Content = new StackPanel
                {
                    Spacing = 1,
                    Children =
                    {
                        new TextBlock { Text = match.Artist.Name },
                        Secondary(match.Artist),
                    },
                },
            };
            button.Click += (_, _) =>
            {
                SetExplicit(_assignments.Append(
                    new BridgeArtistAssignment.Existing(match.Artist)).ToList());
                _query.Text = string.Empty;
                _results.Children.Clear();
            };
            _results.Children.Add(button);
        }
    }

    private static TextBlock Secondary(BridgeExistingArtist artist)
    {
        var text = new TextBlock
        {
            Text = artist.ArtistId,
            FontSize = 11,
        };
        text[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        return text;
    }

    private void AddTyped()
    {
        var name = (_query.Text ?? string.Empty).Trim();
        if (name.Length == 0)
        {
            return;
        }
        SetExplicit(_assignments.Append(new BridgeArtistAssignment.New(
            new BridgeNewArtistSeed(name, null, null, null))).ToList());
        _query.Text = string.Empty;
        _results.Children.Clear();
    }

    private void SetExplicit(List<BridgeArtistAssignment> assignments)
    {
        _inheritsAlbumArtists = false;
        _assignments = assignments;
        _onChange(assignments);
        RenderTokens();
        RenderField();
    }
}

internal static class ArtistAssignmentDisplay
{
    internal static string Name(BridgeArtistAssignment assignment) => assignment switch
    {
        BridgeArtistAssignment.Existing existing => existing.Artist.Name,
        BridgeArtistAssignment.New created => created.Seed.Name,
        _ => throw new ArgumentOutOfRangeException(
            nameof(assignment), assignment, "Unknown artist assignment"),
    };

    internal static string Join(IEnumerable<BridgeArtistAssignment> assignments) =>
        string.Join(
            $"{CultureInfo.CurrentCulture.TextInfo.ListSeparator} ",
            assignments.Select(Name));

    internal static Control Label(BridgeArtistAssignment assignment)
    {
        var name = new TextBlock
        {
            Text = Name(assignment),
            TextTrimming = TextTrimming.CharacterEllipsis,
            MaxLines = 1,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var identity = new TextBlock
        {
            Text = assignment switch
            {
                BridgeArtistAssignment.Existing _ =>
                    Loc.Chrome("artist.assignments.library"),
                BridgeArtistAssignment.New _ =>
                    Loc.Chrome("artist.assignments.new"),
                _ => throw new ArgumentOutOfRangeException(
                    nameof(assignment), assignment, "Unknown artist assignment"),
            },
            FontSize = 10,
            VerticalAlignment = VerticalAlignment.Center,
        };
        identity[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        var badge = new Border
        {
            Padding = new Avalonia.Thickness(5, 2),
            CornerRadius = new Avalonia.CornerRadius(4),
            Child = identity,
        };
        badge[!Border.BackgroundProperty] =
            new DynamicResourceExtension("BaeElevatedBrush");
        return new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
            ColumnSpacing = 5,
            Children = { name, badge.WithGridColumn(1) },
        };
    }
}

internal static class ArtistAssignmentGridExtensions
{
    internal static T WithGridColumn<T>(this T control, int column) where T : Control
    {
        Grid.SetColumn(control, column);
        return control;
    }
}
