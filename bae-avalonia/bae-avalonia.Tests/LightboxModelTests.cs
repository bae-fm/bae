using System;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Tests the lightbox state model: navigation logic, zoom clamping, chrome
/// visibility, zoom reset on image change, and counter formatting.
/// </summary>
public sealed class LightboxModelTests
{
    // ── Open: null on empty, clamped index, initial zoom at MinZoom ──

    [Fact]
    public void Open_ReturnsNullOnZeroCount()
    {
        var state = LightboxModel.Open(0, 0);
        Assert.Null(state);
    }

    [Fact]
    public void Open_ReturnsNullOnNegativeCount()
    {
        var state = LightboxModel.Open(-1, 0);
        Assert.Null(state);
    }

    [Fact]
    public void Open_ClampsNegativeIndex()
    {
        var state = LightboxModel.Open(5, -10);
        Assert.NotNull(state);
        Assert.Equal(0, state!.Index);
    }

    [Fact]
    public void Open_ClampsExcessiveIndex()
    {
        var state = LightboxModel.Open(5, 10);
        Assert.NotNull(state);
        Assert.Equal(4, state!.Index);
    }

    [Fact]
    public void Open_StartsAtMinZoom()
    {
        var state = LightboxModel.Open(3, 1);
        Assert.Equal(LightboxModel.MinZoom, state.Zoom);
    }

    [Fact]
    public void Open_PreservesCount()
    {
        var state = LightboxModel.Open(7, 0);
        Assert.Equal(7, state.Count);
    }

    // ── CanCycle: true when count > 1, false otherwise ──

    [Fact]
    public void CanCycle_TrueForMultipleItems()
    {
        var state = new LightboxState(3, 0, LightboxModel.MinZoom);
        Assert.True(LightboxModel.CanCycle(state));
    }

    [Fact]
    public void CanCycle_FalseForSingleItem()
    {
        var state = new LightboxState(1, 0, LightboxModel.MinZoom);
        Assert.False(LightboxModel.CanCycle(state));
    }

    // ── Next: wrap-around, no-op when !CanCycle, reset zoom ──

    [Fact]
    public void Next_AdvancesIndex()
    {
        var state = new LightboxState(5, 2, 2.0);
        var next = LightboxModel.Next(state);
        Assert.Equal(3, next.Index);
    }

    [Fact]
    public void Next_WrapsAtEnd()
    {
        var state = new LightboxState(5, 4, 2.0);
        var next = LightboxModel.Next(state);
        Assert.Equal(0, next.Index);
    }

    [Fact]
    public void Next_NoOpWhenSingleItem()
    {
        var state = new LightboxState(1, 0, 2.0);
        var next = LightboxModel.Next(state);
        Assert.Equal(0, next.Index);
    }

    [Fact]
    public void Next_ResetsZoom()
    {
        var state = new LightboxState(5, 2, 4.5);
        var next = LightboxModel.Next(state);
        Assert.Equal(LightboxModel.MinZoom, next.Zoom);
    }

    // ── Previous: wrap-around, no-op when !CanCycle, reset zoom ──

    [Fact]
    public void Previous_DecrementsIndex()
    {
        var state = new LightboxState(5, 2, 2.0);
        var prev = LightboxModel.Previous(state);
        Assert.Equal(1, prev.Index);
    }

    [Fact]
    public void Previous_WrapsAtStart()
    {
        var state = new LightboxState(5, 0, 2.0);
        var prev = LightboxModel.Previous(state);
        Assert.Equal(4, prev.Index);
    }

    [Fact]
    public void Previous_NoOpWhenSingleItem()
    {
        var state = new LightboxState(1, 0, 2.0);
        var prev = LightboxModel.Previous(state);
        Assert.Equal(0, prev.Index);
    }

    [Fact]
    public void Previous_ResetsZoom()
    {
        var state = new LightboxState(5, 2, 4.5);
        var prev = LightboxModel.Previous(state);
        Assert.Equal(LightboxModel.MinZoom, prev.Zoom);
    }

    // ── Full navigation cycle ──

    [Fact]
    public void NavigationCycles()
    {
        var state = LightboxModel.Open(3, 0);
        Assert.NotNull(state);

        state = LightboxModel.Next(state);
        Assert.Equal(1, state.Index);

        state = LightboxModel.Next(state);
        Assert.Equal(2, state.Index);

        state = LightboxModel.Next(state);
        Assert.Equal(0, state.Index);
    }

    // ── Select: in-range moves, out-of-range ignored, same-index preserves zoom ──

    [Fact]
    public void Select_MovesToValidIndex()
    {
        var state = new LightboxState(5, 1, LightboxModel.MinZoom);
        var selected = LightboxModel.Select(state, 3);
        Assert.Equal(3, selected.Index);
    }

    [Fact]
    public void Select_IgnoresNegativeIndex()
    {
        var state = new LightboxState(5, 1, LightboxModel.MinZoom);
        var selected = LightboxModel.Select(state, -1);
        Assert.Equal(1, selected.Index);
    }

    [Fact]
    public void Select_IgnoresExcessiveIndex()
    {
        var state = new LightboxState(5, 1, LightboxModel.MinZoom);
        var selected = LightboxModel.Select(state, 10);
        Assert.Equal(1, selected.Index);
    }

    [Fact]
    public void Select_ResetsZoomWhenMoving()
    {
        var state = new LightboxState(5, 1, 3.5);
        var selected = LightboxModel.Select(state, 3);
        Assert.Equal(LightboxModel.MinZoom, selected.Zoom);
    }

    [Fact]
    public void Select_PreservesZoomWhenSameIndex()
    {
        var state = new LightboxState(5, 1, 3.5);
        var selected = LightboxModel.Select(state, 1);
        Assert.Equal(3.5, selected.Zoom);
    }

    // ── ClampZoom: pins to [MinZoom, MaxZoom] ──

    [Fact]
    public void ClampZoom_BelowMinimum()
    {
        var clamped = LightboxModel.ClampZoom(0.5);
        Assert.Equal(LightboxModel.MinZoom, clamped);
    }

    [Fact]
    public void ClampZoom_AboveMaximum()
    {
        var clamped = LightboxModel.ClampZoom(10.0);
        Assert.Equal(LightboxModel.MaxZoom, clamped);
    }

    [Fact]
    public void ClampZoom_WithinRange()
    {
        var clamped = LightboxModel.ClampZoom(3.5);
        Assert.Equal(3.5, clamped);
    }

    // ── DoubleTapTarget: zoom to 2.5 from 1.0, reset to 1.0 from zoomed ──

    [Fact]
    public void DoubleTapTarget_FromMinZoom()
    {
        var target = LightboxModel.DoubleTapTarget(LightboxModel.MinZoom);
        Assert.Equal(LightboxModel.DoubleTapZoom, target);
    }

    [Fact]
    public void DoubleTapTarget_FromBelowThreshold()
    {
        var target = LightboxModel.DoubleTapTarget(1.0);
        Assert.Equal(LightboxModel.DoubleTapZoom, target);
    }

    [Fact]
    public void DoubleTapTarget_FromAtThreshold()
    {
        var target = LightboxModel.DoubleTapTarget(1.01);
        Assert.Equal(LightboxModel.DoubleTapZoom, target);
    }

    [Fact]
    public void DoubleTapTarget_FromAboveThreshold()
    {
        var target = LightboxModel.DoubleTapTarget(3.0);
        Assert.Equal(LightboxModel.MinZoom, target);
    }

    [Fact]
    public void DoubleTapTarget_FromMaxZoom()
    {
        var target = LightboxModel.DoubleTapTarget(LightboxModel.MaxZoom);
        Assert.Equal(LightboxModel.MinZoom, target);
    }

    // ── ChromeVisible: true at <= 1.01, false above ──

    [Fact]
    public void ChromeVisible_AtMinZoom()
    {
        Assert.True(LightboxModel.ChromeVisible(LightboxModel.MinZoom));
    }

    [Fact]
    public void ChromeVisible_AtThreshold()
    {
        Assert.True(LightboxModel.ChromeVisible(1.01));
    }

    [Fact]
    public void ChromeVisible_SlightlyAboveThreshold()
    {
        Assert.False(LightboxModel.ChromeVisible(1.02));
    }

    [Fact]
    public void ChromeVisible_AtMaxZoom()
    {
        Assert.False(LightboxModel.ChromeVisible(LightboxModel.MaxZoom));
    }

    // ── Counter: returns gallery.counter key with 1-based index ──

    [Fact]
    public void Counter_ReturnsGalleryCounterKey()
    {
        var state = new LightboxState(5, 2, LightboxModel.MinZoom);
        var (key, args) = LightboxModel.Counter(state);
        Assert.Equal("gallery.counter", key);
    }

    [Fact]
    public void Counter_IndexIs1Based()
    {
        var state = new LightboxState(5, 2, LightboxModel.MinZoom);
        var (_, args) = LightboxModel.Counter(state);
        Assert.Equal(3, args["index"]);
    }

    [Fact]
    public void Counter_IncludesCount()
    {
        var state = new LightboxState(5, 2, LightboxModel.MinZoom);
        var (_, args) = LightboxModel.Counter(state);
        Assert.Equal(5, args["count"]);
    }

    [Fact]
    public void Counter_FirstItem()
    {
        var state = new LightboxState(3, 0, LightboxModel.MinZoom);
        var (_, args) = LightboxModel.Counter(state);
        Assert.Equal(1, args["index"]);
        Assert.Equal(3, args["count"]);
    }
}
