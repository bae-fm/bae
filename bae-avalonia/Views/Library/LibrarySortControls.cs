using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The library sort surface, rendered into the header band opposite the mode
// heading: a pill per sort criterion (its field, whose menu re-points it at
// another field; an arrow that inverts its direction; an "✕" that removes it)
// and a "+" whose menu adds an unused field. The same pill surface renders for all three browse modes over each mode's
// own criteria list. Every mutation goes through the LibrarySort model, which
// raises Changed — the store reloads the affected list off that; this only builds
// controls and re-renders itself, so it stays a thin renderer of the model.
internal sealed class LibrarySortControls
{
    private readonly Panel _host;
    private readonly LibrarySort _sort;

    public LibrarySortControls(Panel host, LibrarySort sort)
    {
        _host = host;
        _sort = sort;
    }

    // Rebuild the controls for the active mode. Called on mode changes and after
    // every mutation (add/remove change the control set; a direction change flips
    // its pill's arrow).
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

    // A criterion pill in three parts. The field name opens a menu of every
    // field: the current one is checked, one another pill holds is disabled,
    // and choosing one re-points this criterion in place — so changing what a
    // single pill sorts by is one pick, not a remove and an add. The arrow sets
    // the opposite direction — an absolute set computed from the rendered
    // direction, not a blind toggle. A trailing "✕" removes it, shown only when
    // more than one remains.
    private Control BuildPill<TField>(SortCriteria<TField> criteria, SortCriterion<TField> criterion)
        where TField : struct, Enum
    {
        var label = Loc.Chrome(criteria.Vocab.LabelKey(criterion.Field));

        var pill = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 6,
        };
        pill.Children.Add(BuildFieldButton(criteria, criterion, label));
        pill.Children.Add(BuildDirectionButton(criteria, criterion, label));
        if (criteria.CanRemove)
        {
            pill.Children.Add(BuildRemoveButton(criteria, criterion));
        }
        return pill;
    }

    private Button BuildFieldButton<TField>(
        SortCriteria<TField> criteria,
        SortCriterion<TField> criterion,
        string label)
        where TField : struct, Enum
    {
        var items = new List<MenuItem>();
        foreach (var field in criteria.Vocab.CanonicalFields)
        {
            var target = field;
            var isCurrent = EqualityComparer<TField>.Default.Equals(field, criterion.Field);
            var item = new MenuItem
            {
                Header = Loc.Chrome(criteria.Vocab.LabelKey(field)),
                ToggleType = MenuItemToggleType.CheckBox,
                IsChecked = isCurrent,
                // Another pill already sorts by it; that pill is where it lives.
                IsEnabled = isCurrent || criteria.AvailableToAdd.Contains(field),
            };
            item.Click += (_, _) =>
            {
                criteria.SetField(criterion.Field, target);
                Render();
            };
            items.Add(item);
        }

        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 5 };
        var labelText = new TextBlock
        {
            Text = label,
            FontSize = 13,
            FontWeight = FontWeight.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        labelText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var chevron = new TextBlock
        {
            Text = "⌄",
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        chevron[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        content.Children.Add(labelText);
        content.Children.Add(chevron);

        var button = Pill(content, new Thickness(12, 7));
        button.Flyout = new MenuFlyout { ItemsSource = items };
        Avalonia.Automation.AutomationProperties.SetName(button, label);
        ToolTip.SetTip(button, Loc.Chrome("sort.criterion.field"));
        return button;
    }

    private Button BuildDirectionButton<TField>(
        SortCriteria<TField> criteria,
        SortCriterion<TField> criterion,
        string label)
        where TField : struct, Enum
    {
        var arrow = criterion.Direction == SortDirection.Ascending ? "↑" : "↓";
        var opposite = LibrarySortVocab.Opposite(criterion.Direction);
        var arrowText = new TextBlock
        {
            Text = arrow,
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        arrowText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        var toggle = Pill(arrowText, new Thickness(9, 7));
        var action = Loc.Chrome(LibrarySortVocab.DirectionActionKey(opposite));
        Avalonia.Automation.AutomationProperties.SetName(toggle, $"{label}: {action}");
        ToolTip.SetTip(toggle, action);
        toggle.Click += (_, _) =>
        {
            criteria.SetDirection(criterion.Field, opposite);
            Render();
        };
        return toggle;
    }

    private Button BuildRemoveButton<TField>(SortCriteria<TField> criteria, SortCriterion<TField> criterion)
        where TField : struct, Enum
    {
        var remove = Pill(new TextBlock { Text = "✕", FontSize = 9 }, new Thickness(7));
        Avalonia.Automation.AutomationProperties.SetName(remove, Loc.Chrome("sort.criterion.remove"));
        ToolTip.SetTip(remove, Loc.Chrome("sort.criterion.remove"));
        remove.Click += (_, _) =>
        {
            criteria.Remove(criterion.Field);
            Render();
        };
        return remove;
    }

    // The "+" menu of unused fields, present only when a field remains to add.
    private Button BuildAddButton<TField>(SortCriteria<TField> criteria, IReadOnlyList<TField> addable)
        where TField : struct, Enum
    {
        var items = new List<MenuItem>();
        foreach (var field in addable)
        {
            var target = field;
            var item = new MenuItem { Header = Loc.Chrome(criteria.Vocab.LabelKey(field)) };
            item.Click += (_, _) =>
            {
                criteria.Add(target);
                Render();
            };
            items.Add(item);
        }

        var button = Pill(new TextBlock { Text = "+", FontSize = 13 }, new Thickness(9, 6));
        button.Flyout = new MenuFlyout { ItemsSource = items };
        Avalonia.Automation.AutomationProperties.SetName(button, Loc.Chrome("sort.criterion.add"));
        ToolTip.SetTip(button, Loc.Chrome("sort.criterion.add"));
        return button;
    }

    private static Button Pill(Control content, Thickness padding)
    {
        var button = new Button
        {
            Content = content,
            Padding = padding,
            CornerRadius = new CornerRadius(9),
            BorderThickness = new Thickness(1),
            VerticalAlignment = VerticalAlignment.Center,
        };
        button[!Button.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        button[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        return button;
    }
}
