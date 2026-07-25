using System;

namespace Bae.Desktop;

// The library header's collapse-on-scroll math, isolated from the UI so it can be
// tested. Progress is a pure function of how far the active scrollable has
// scrolled: 0 at rest, rising linearly to 1 once the scroll passes the track
// distance, so the heading scrubs from full to compact. When scrolling settles
// mid-scrub, it snaps to the nearer end. Only the most-recently-scrolled surface
// drives it, so switching browse panels — each with its own scrollable — doesn't
// leave a stale surface fighting the heading.
internal sealed class HeaderCollapseModel
{
    // The scroll distance over which the heading fully collapses.
    public const double TrackDistance = 64;

    private string? _activeScroller;

    // The current collapse fraction, 0 (full) to 1 (compact).
    public double Progress { get; private set; }

    // Report a scroll offset from a named surface, which becomes the active driver.
    // Returns the resulting progress.
    public double ReportScroll(string scroller, double offset)
    {
        _activeScroller = scroller;
        Progress = Math.Clamp(offset / TrackDistance, 0, 1);
        return Progress;
    }

    // Report that a surface stopped scrolling. When the active surface settles
    // mid-scrub, progress snaps to the nearer end; a report from a non-active
    // surface, or one already resolved to an end, is ignored. Returns true when the
    // snap moved progress, so the caller animates to the settled value.
    public bool ReportSettled(string scroller)
    {
        if (scroller != _activeScroller || Progress <= 0 || Progress >= 1)
        {
            return false;
        }
        Progress = Progress < 0.5 ? 0 : 1;
        return true;
    }
}
