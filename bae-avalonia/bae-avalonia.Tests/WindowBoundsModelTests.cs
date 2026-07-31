using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the pure window-bounds logic: the serialize/parse round-trip, the
/// validation that rejects garbage or degenerate saved rects, and the clamping
/// that keeps a restored window reachable when the display arrangement shrank or
/// a monitor went away. Runs with no windowing platform up: the display
/// arrangement is passed in as plain rects.
/// </summary>
public sealed class WindowBoundsModelTests
{
    // Single-display arrangement: one work area starting at the origin.
    private static readonly PixelRect[] SingleDisplay = { new(0, 0, 1920, 1040) };

    // Multi-display arrangement: primary first, then a display to its right and a
    // negative-origin display to its left.
    private static readonly PixelRect[] MultiDisplay =
    {
        new(0, 0, 1920, 1040),
        new(1920, 0, 1920, 1160),
        new(-1280, 0, 1280, 1024),
    };

    // ── Round-trip / valid restore ──

    [Fact]
    public void PlanRestore_RoundTripsBoundsAndFlag()
    {
        var bounds = new PixelRect(100, 80, 1100, 800);
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(bounds, maximized: false),
            SingleDisplay);

        Assert.NotNull(plan);
        Assert.Equal(bounds, plan!.Bounds);
        Assert.False(plan.Maximized);
    }

    [Fact]
    public void PlanRestore_StraddlingTwoDisplaysStaysPut()
    {
        // Spans the seam between the primary and the display to its right, with a
        // grabbable title bar on both — restored exactly where it was.
        var bounds = new PixelRect(1600, 100, 900, 700);
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(bounds, maximized: false),
            MultiDisplay);

        Assert.NotNull(plan);
        Assert.Equal(bounds, plan!.Bounds);
    }

    [Fact]
    public void PlanRestore_NegativeOriginDisplayRestoresUnmoved()
    {
        // A rect fully on the negative-origin display is not off-screen.
        var bounds = new PixelRect(-1000, 100, 800, 600);
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(bounds, maximized: false),
            MultiDisplay);

        Assert.NotNull(plan);
        Assert.Equal(bounds, plan!.Bounds);
    }

    // ── Off-screen clamp ──

    [Fact]
    public void PlanRestore_RectOnUnpluggedDisplayClampsIntoFallback()
    {
        // Saved on a display that is no longer present; only the primary remains.
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(new PixelRect(2200, 100, 800, 600), maximized: false),
            SingleDisplay);

        Assert.NotNull(plan);
        AssertInside(plan!.Bounds, SingleDisplay[0]);
        Assert.Equal(800, plan.Bounds.Width);
        Assert.Equal(600, plan.Bounds.Height);
    }

    [Fact]
    public void PlanRestore_TitleBarAboveWorkAreaClamps()
    {
        // The top edge sits above the display, so the title bar is unreachable.
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(new PixelRect(100, -200, 800, 600), maximized: false),
            SingleDisplay);

        Assert.NotNull(plan);
        AssertInside(plan!.Bounds, SingleDisplay[0]);
        Assert.True(plan.Bounds.Y >= SingleDisplay[0].Y);
    }

    [Fact]
    public void PlanRestore_HorizontalSliverClamps()
    {
        // Only a thin sliver of width overlaps the display (less than the minimum
        // visible width), so the window is pulled back on-screen.
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(new PixelRect(1870, 100, 800, 600), maximized: false),
            SingleDisplay);

        Assert.NotNull(plan);
        AssertInside(plan!.Bounds, SingleDisplay[0]);
    }

    // ── Monitor-shrink clamp ──

    [Fact]
    public void PlanRestore_OversizedRectShrinksToWorkArea()
    {
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(new PixelRect(0, 0, 2560, 1400), maximized: false),
            SingleDisplay);

        Assert.NotNull(plan);
        Assert.Equal(1920, plan!.Bounds.Width);
        Assert.Equal(1040, plan.Bounds.Height);
        AssertInside(plan.Bounds, SingleDisplay[0]);
    }

    [Fact]
    public void PlanRestore_FitsBySizeButTitleBarBelowShrunkDisplayRepositions()
    {
        // Fits by size, but after the display got shorter the title bar sits below
        // its bottom edge (unreachable) — repositioned to sit fully inside.
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(new PixelRect(1400, 1010, 800, 300), maximized: false),
            SingleDisplay);

        Assert.NotNull(plan);
        AssertInside(plan!.Bounds, SingleDisplay[0]);
        Assert.Equal(800, plan.Bounds.Width);
        Assert.Equal(300, plan.Bounds.Height);
    }

    // ── Garbage / corrupt saved values → null plan ──

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData("not json")]
    [InlineData("[1, 2, 3]")]
    [InlineData("{\"x\":100,\"y\":80}")]
    [InlineData("{\"x\":\"100\",\"y\":\"80\",\"width\":\"800\",\"height\":\"600\"}")]
    [InlineData("{\"x\":0,\"y\":0,\"width\":0,\"height\":600}")]
    [InlineData("{\"x\":0,\"y\":0,\"width\":-800,\"height\":600}")]
    [InlineData("{\"x\":0,\"y\":0,\"width\":50,\"height\":600}")]
    [InlineData("{\"x\":0,\"y\":0,\"width\":800,\"height\":100}")]
    public void PlanRestore_GarbageReturnsNull(string? json) =>
        Assert.Null(WindowBoundsModel.PlanRestore(json, SingleDisplay));

    [Fact]
    public void PlanRestore_EmptyWorkAreasReturnsNull() =>
        Assert.Null(WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(new PixelRect(100, 80, 800, 600), maximized: false),
            new PixelRect[0]));

    [Fact]
    public void PlanRestore_AbsentMaximizedReadsAsFalse()
    {
        var plan = WindowBoundsModel.PlanRestore(
            "{\"x\":100,\"y\":80,\"width\":800,\"height\":600}",
            SingleDisplay);

        Assert.NotNull(plan);
        Assert.False(plan!.Maximized);
    }

    [Fact]
    public void PlanRestore_NonBooleanMaximizedReadsAsFalse()
    {
        var plan = WindowBoundsModel.PlanRestore(
            "{\"x\":100,\"y\":80,\"width\":800,\"height\":600,\"maximized\":\"yes\"}",
            SingleDisplay);

        Assert.NotNull(plan);
        Assert.False(plan!.Maximized);
    }

    // ── Maximized round-trip ──

    [Fact]
    public void PlanRestore_MaximizedPreservesNormalBounds()
    {
        var normal = new PixelRect(200, 150, 1100, 800);
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(normal, maximized: true),
            SingleDisplay);

        Assert.NotNull(plan);
        Assert.True(plan!.Maximized);
        Assert.Equal(normal, plan.Bounds);
    }

    [Fact]
    public void PlanRestore_MaximizedWithOffScreenNormalBoundsClamps()
    {
        var plan = WindowBoundsModel.PlanRestore(
            WindowBoundsModel.Serialize(new PixelRect(2200, 100, 800, 600), maximized: true),
            SingleDisplay);

        Assert.NotNull(plan);
        Assert.True(plan!.Maximized);
        AssertInside(plan.Bounds, SingleDisplay[0]);
    }

    private static void AssertInside(PixelRect rect, PixelRect workArea)
    {
        Assert.True(rect.X >= workArea.X, "left edge inside");
        Assert.True(rect.Y >= workArea.Y, "top edge inside");
        Assert.True(rect.Right <= workArea.Right, "right edge inside");
        Assert.True(rect.Bottom <= workArea.Bottom, "bottom edge inside");
    }
}
