using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Bae.Windows;

// Renders the library sort surface into the toolbar: for albums, a chip per sort
// criterion (each a button whose flyout toggles direction, reorders, removes, and
// switches field) plus a "+" button that adds an unused field; for composers, a
// field button and a direction-toggle button. Every action mutates the LibrarySort
// model and asks the window to reload. The window owns visibility and reload; this
// only builds controls, so it stays a thin renderer of the model.
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
        for (var index = 0; index < items.Count; index++)
        {
            _host.Children.Add(BuildAlbumChip(items[index], index, items.Count));
        }

        var addable = _sort.Albums.AvailableToAdd;
        if (addable.Count > 0)
        {
            _host.Children.Add(BuildAddButton(addable));
        }
    }

    private Button BuildAlbumChip(AlbumSortCriterion criterion, int index, int count)
    {
        var arrow = criterion.Direction == SortDirection.Ascending ? AscendingArrow : DescendingArrow;
        var menu = new MenuFlyout();

        var opposite = LibrarySortVocab.Opposite(criterion.Direction);
        var toggle = new MenuFlyoutItem { Text = Loc.Chrome(LibrarySortVocab.DirectionActionKey(opposite)) };
        toggle.Click += (_, _) =>
        {
            _sort.Albums.SetDirection(criterion.Field, opposite);
            Mutated();
        };
        menu.Items.Add(toggle);

        if (index > 0)
        {
            var moveUp = new MenuFlyoutItem { Text = Loc.Chrome("sort.criterion.move_up") };
            moveUp.Click += (_, _) =>
            {
                _sort.Albums.MoveUp(criterion.Field);
                Mutated();
            };
            menu.Items.Add(moveUp);
        }

        if (index < count - 1)
        {
            var moveDown = new MenuFlyoutItem { Text = Loc.Chrome("sort.criterion.move_down") };
            moveDown.Click += (_, _) =>
            {
                _sort.Albums.MoveDown(criterion.Field);
                Mutated();
            };
            menu.Items.Add(moveDown);
        }

        if (_sort.Albums.CanRemove)
        {
            var remove = new MenuFlyoutItem { Text = Loc.Chrome("action.remove") };
            remove.Click += (_, _) =>
            {
                _sort.Albums.Remove(criterion.Field);
                Mutated();
            };
            menu.Items.Add(remove);
        }

        menu.Items.Add(new MenuFlyoutSeparator());

        foreach (var field in _sort.Albums.ChoosableFieldsFor(criterion.Field))
        {
            var item = new MenuFlyoutItem { Text = Loc.Chrome(LibrarySortVocab.LabelKey(field)) };
            if (field == criterion.Field)
            {
                item.Icon = new FontIcon { Glyph = CheckmarkGlyph };
            }

            var target = field;
            item.Click += (_, _) =>
            {
                _sort.Albums.SetField(criterion.Field, target);
                Mutated();
            };
            menu.Items.Add(item);
        }

        return new Button
        {
            Content = $"{Loc.Chrome(LibrarySortVocab.LabelKey(criterion.Field))} {arrow}",
            VerticalAlignment = VerticalAlignment.Center,
            Flyout = menu,
        };
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
