using System;
using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Bae.Windows;

// Renders the library sort surface into the toolbar: a pill per sort criterion
// (a button that inverts that field's direction, plus an "x" button that
// removes it) and a "+" button whose flyout adds an unused field. The same pill
// surface renders for all three browser modes, over each mode's own criteria
// list. Field switching and reordering are gone by design — the sort is built
// by adding and removing pills. Every action mutates the LibrarySort model and
// asks the window to reload. The window owns visibility and reload; this only
// builds controls, so it stays a thin renderer of the model.
internal sealed class LibrarySortControls
{
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
    // every mutation (add/remove change the control set, direction changes its
    // pill's label).
    public void Render()
    {
        _host.Children.Clear();
        switch (_sort.Mode)
        {
            case BrowserMode.Composers:
                RenderCriteria(_sort.Composers);
                break;
            case BrowserMode.Artists:
                RenderCriteria(_sort.Artists);
                break;
            default:
                RenderCriteria(_sort.Albums);
                break;
        }
    }

    private void Mutated()
    {
        Render();
        _reload();
    }

    private void RenderCriteria<TField>(SortCriteria<TField> criteria) where TField : struct, Enum
    {
        foreach (var criterion in criteria.Items)
        {
            _host.Children.Add(BuildPill(criteria, criterion));
        }

        var addable = criteria.AvailableToAdd;
        if (addable.Count > 0)
        {
            _host.Children.Add(BuildAddButton(criteria, addable));
        }
    }

    // A sort criterion pill: the main button (field label + direction arrow)
    // sets the opposite direction — an absolute set computed from the rendered
    // direction, not a blind toggle. The trailing "x" removes the criterion; on
    // the last remaining criterion it is absent, not disabled. The two buttons
    // share a pill silhouette through their corner radii.
    private UIElement BuildPill<TField>(SortCriteria<TField> criteria, SortCriterion<TField> criterion)
        where TField : struct, Enum
    {
        var arrow = criterion.Direction == SortDirection.Ascending ? AscendingArrow : DescendingArrow;
        var opposite = LibrarySortVocab.Opposite(criterion.Direction);

        var toggle = new Button
        {
            Content = $"{Loc.Chrome(criteria.Vocab.LabelKey(criterion.Field))} {arrow}",
            VerticalAlignment = VerticalAlignment.Center,
            CornerRadius = new CornerRadius(14),
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            toggle, Loc.Chrome(criteria.Vocab.LabelKey(criterion.Field)));
        ToolTipService.SetToolTip(toggle, Loc.Chrome(LibrarySortVocab.DirectionActionKey(opposite)));
        toggle.Click += (_, _) =>
        {
            criteria.SetDirection(criterion.Field, opposite);
            Mutated();
        };

        if (!criteria.CanRemove)
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
            criteria.Remove(criterion.Field);
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

    private Button BuildAddButton<TField>(SortCriteria<TField> criteria, IReadOnlyList<TField> addable)
        where TField : struct, Enum
    {
        var menu = new MenuFlyout();
        foreach (var field in addable)
        {
            var item = new MenuFlyoutItem { Text = Loc.Chrome(criteria.Vocab.LabelKey(field)) };
            var target = field;
            item.Click += (_, _) =>
            {
                criteria.Add(target);
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
}
