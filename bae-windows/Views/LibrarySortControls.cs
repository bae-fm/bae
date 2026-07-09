using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Bae.Windows;

// Renders the library sort surface into the toolbar: for albums, a pill per sort
// criterion (a button that inverts that field's direction, plus an "x" button
// that removes it) and a "+" button whose flyout adds an unused field; for
// composers, a field-picker button and a direction-invert button. Field
// switching and reordering are gone by design — the sort is built by adding and
// removing pills. Every action mutates the LibrarySort model and asks the window
// to reload. The window owns visibility and reload; this only builds controls,
// so it stays a thin renderer of the model.
internal sealed class LibrarySortControls
{
    private const string CheckmarkGlyph = "";
    private const string AscendingArrow = "↑";
    private const string DescendingArrow = "↓";

    private readonly Panel _host;
    private readonly LibrarySort _sort;
    private readonly Action _reload;

    public LibrarySortControls(Panel host, LibrarySort sort, Action reload)
    {
        _host = host;
        _sort = sort;
        _reload = reload;
    }

    // Rebuild the controls for the active mode. Called on mode changes and after
    // every mutation (add/remove/reorder change the control set, direction/field
    // change their labels).
    public void Render()
    {
        _host.Children.Clear();
        if (_sort.Mode == BrowserMode.Composers)
        {
            RenderComposer();
        }
        else
        {
            RenderAlbums();
        }
    }

    private void Mutated()
    {
        Render();
        _reload();
    }

    private void RenderAlbums()
    {
        var items = _sort.Albums.Items;
        foreach (var criterion in items)
        {
            _host.Children.Add(BuildAlbumPill(criterion));
        }

        var addable = _sort.Albums.AvailableToAdd;
        if (addable.Count > 0)
        {
            _host.Children.Add(BuildAddButton(addable));
        }
    }

    // A sort criterion pill: the main button (field label + direction arrow)
    // sets the opposite direction — an absolute set computed from the rendered
    // direction, not a blind toggle. The trailing "x" removes the criterion; on
    // the last remaining criterion it is absent, not disabled. The two buttons
    // share a pill silhouette through their corner radii.
    private UIElement BuildAlbumPill(AlbumSortCriterion criterion)
    {
        var arrow = criterion.Direction == SortDirection.Ascending ? AscendingArrow : DescendingArrow;
        var opposite = LibrarySortVocab.Opposite(criterion.Direction);

        var toggle = new Button
        {
            Content = $"{Loc.Chrome(LibrarySortVocab.LabelKey(criterion.Field))} {arrow}",
            VerticalAlignment = VerticalAlignment.Center,
            CornerRadius = new CornerRadius(14),
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            toggle, Loc.Chrome(LibrarySortVocab.LabelKey(criterion.Field)));
        ToolTipService.SetToolTip(toggle, Loc.Chrome(LibrarySortVocab.DirectionActionKey(opposite)));
        toggle.Click += (_, _) =>
        {
            _sort.Albums.SetDirection(criterion.Field, opposite);
            Mutated();
        };

        if (!_sort.Albums.CanRemove)
        {
            return toggle;
        }

        toggle.CornerRadius = new CornerRadius(14, 0, 0, 14);

        var remove = new Button
        {
            // Segoe MDL2 Assets "ChromeClose" glyph (U+E711).
            Content = new FontIcon { Glyph = "\uE711", FontSize = 10 },
            VerticalAlignment = VerticalAlignment.Center,
            CornerRadius = new CornerRadius(0, 14, 14, 0),
            Padding = new Thickness(6, 5, 8, 5),
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            remove, Loc.Chrome("sort.criterion.remove"));
        ToolTipService.SetToolTip(remove, Loc.Chrome("sort.criterion.remove"));
        remove.Click += (_, _) =>
        {
            _sort.Albums.Remove(criterion.Field);
            Mutated();
        };

        var pill = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 1,
        };
        pill.Children.Add(toggle);
        pill.Children.Add(remove);
        return pill;
    }

    private Button BuildAddButton(System.Collections.Generic.IReadOnlyList<AlbumSortField> addable)
    {
        var menu = new MenuFlyout();
        foreach (var field in addable)
        {
            var item = new MenuFlyoutItem { Text = Loc.Chrome(LibrarySortVocab.LabelKey(field)) };
            var target = field;
            item.Click += (_, _) =>
            {
                _sort.Albums.Add(target);
                Mutated();
            };
            menu.Items.Add(item);
        }

        var button = new Button
        {
            Content = "+",
            VerticalAlignment = VerticalAlignment.Center,
            Flyout = menu,
        };
        ToolTipService.SetToolTip(button, Loc.Chrome("sort.criterion.add"));
        return button;
    }

    private void RenderComposer()
    {
        var composer = _sort.Composer;

        var fieldMenu = new MenuFlyout();
        foreach (var field in LibrarySortVocab.ComposerFields)
        {
            var item = new MenuFlyoutItem { Text = Loc.Chrome(LibrarySortVocab.LabelKey(field)) };
            if (field == composer.Field)
            {
                item.Icon = new FontIcon { Glyph = CheckmarkGlyph };
            }

            var target = field;
            item.Click += (_, _) =>
            {
                _sort.SetComposer(target, composer.Direction);
                Mutated();
            };
            fieldMenu.Items.Add(item);
        }

        _host.Children.Add(new Button
        {
            Content = Loc.Chrome(LibrarySortVocab.LabelKey(composer.Field)),
            VerticalAlignment = VerticalAlignment.Center,
            Flyout = fieldMenu,
        });

        var opposite = LibrarySortVocab.Opposite(composer.Direction);
        var arrow = composer.Direction == SortDirection.Ascending ? AscendingArrow : DescendingArrow;
        var directionButton = new Button
        {
            Content = arrow,
            VerticalAlignment = VerticalAlignment.Center,
            CornerRadius = new CornerRadius(14),
        };
        ToolTipService.SetToolTip(directionButton, Loc.Chrome(LibrarySortVocab.DirectionActionKey(opposite)));
        directionButton.Click += (_, _) =>
        {
            _sort.SetComposer(composer.Field, opposite);
            Mutated();
        };
        _host.Children.Add(directionButton);
    }
}
