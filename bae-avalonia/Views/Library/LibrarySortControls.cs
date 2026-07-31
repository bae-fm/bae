using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The library sort surface, rendered into the header band opposite the mode
// heading: a pill per sort criterion (a button that inverts that field's
// direction, plus an "✕" that removes it) and a "+" whose menu adds an unused
// field. The same pill surface renders for all three browse modes over each mode's
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

    // A criterion pill: the main button (field label + direction arrow) sets the
    // opposite direction — an absolute set computed from the rendered direction,
    // not a blind toggle. A trailing "✕" removes it, shown only when more than one
    // remains.
    private Control BuildPill<TField>(SortCriteria<TField> criteria, SortCriterion<TField> criterion)
        where TField : struct, Enum
    {
        var arrow = criterion.Direction == SortDirection.Ascending ? "↑" : "↓";
        var opposite = LibrarySortVocab.Opposite(criterion.Direction);
        var label = Loc.Chrome(criteria.Vocab.LabelKey(criterion.Field));

        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 5 };
        var labelText = new TextBlock
        {
            Text = label,
            FontSize = 13,
            FontWeight = FontWeight.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        labelText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var arrowText = new TextBlock
        {
            Text = arrow,
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        arrowText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        content.Children.Add(labelText);
        content.Children.Add(arrowText);

        var toggle = Pill(content, new Thickness(12, 7));
        Avalonia.Automation.AutomationProperties.SetName(toggle, label);
        ToolTip.SetTip(toggle, Loc.Chrome(LibrarySortVocab.DirectionActionKey(opposite)));
        toggle.Click += (_, _) =>
        {
            criteria.SetDirection(criterion.Field, opposite);
            Render();
        };

        if (!criteria.CanRemove)
        {
            return toggle;
        }

        var remove = Pill(new TextBlock { Text = "✕", FontSize = 9 }, new Thickness(7));
        Avalonia.Automation.AutomationProperties.SetName(remove, Loc.Chrome("sort.criterion.remove"));
        ToolTip.SetTip(remove, Loc.Chrome("sort.criterion.remove"));
        remove.Click += (_, _) =>
        {
            criteria.Remove(criterion.Field);
            Render();
        };

        var pill = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 6,
        };
        pill.Children.Add(toggle);
        pill.Children.Add(remove);
        return pill;
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
