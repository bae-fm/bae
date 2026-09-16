using System;
using Avalonia.Controls;
using Avalonia.Threading;

namespace Bae.Desktop;

/// <summary>
/// Keeps a hover-opened flyout up while the pointer crosses the gap between the
/// control it hangs off and the flyout's own content.
///
/// Both the anchor and the content schedule the close, and either one being
/// entered cancels it, so the pointer leaving the anchor on its way into the
/// flyout never closes it. The delay is what makes that crossing possible at
/// all — an immediate close would fire the moment the pointer left the anchor's
/// bounds.
/// </summary>
internal static class HoverFlyout
{
    /// <summary>How long the pointer may be outside both the anchor and the
    /// content before the flyout closes.</summary>
    private static readonly TimeSpan Debounce = TimeSpan.FromSeconds(0.3);

    /// <summary>Hang a flyout off <paramref name="anchor"/>, built by
    /// <paramref name="content"/> the first time it opens. Placed above the
    /// anchor, so the anchor stays visible under it.</summary>
    internal static void Attach(Control anchor, Func<Control> content)
    {
        var flyout = new Flyout
        {
            Placement = PlacementMode.Top,
            ShowMode = FlyoutShowMode.Transient,
        };
        var timer = new DispatcherTimer { Interval = Debounce };
        timer.Tick += (_, _) =>
        {
            timer.Stop();
            flyout.Hide();
        };

        void Enter()
        {
            timer.Stop();
            if (flyout.Content is null)
            {
                var built = content();
                built.PointerEntered += (_, _) => Enter();
                built.PointerExited += (_, _) => Leave();
                flyout.Content = built;
            }
            flyout.ShowAt(anchor);
        }

        void Leave()
        {
            timer.Stop();
            timer.Start();
        }

        anchor.PointerEntered += (_, _) => Enter();
        anchor.PointerExited += (_, _) => Leave();
        anchor.DetachedFromVisualTree += (_, _) =>
        {
            timer.Stop();
            flyout.Hide();
        };
    }
}
