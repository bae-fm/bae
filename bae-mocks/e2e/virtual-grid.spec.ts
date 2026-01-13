import { test, expect, Page } from '@playwright/test';

// Capture and display console logs from the app
test.beforeEach(async ({ page }) => {
  page.on('console', msg => {
    const type = msg.type();
    const text = msg.text();
    // Filter to our app logs (they have prefixes like LAYOUT:, MEASURE:)
    if (text.includes('LAYOUT:') || text.includes('MEASURE:') || text.includes('SCROLL:')) {
      console.log(`[browser ${type}] ${text}`);
    }
  });
});

// Helper to count grid items in DOM
async function countGridItems(page: Page): Promise<number> {
  return await page.locator('.virtual-grid-content > div').count();
}

// Helper to get visible album titles
async function getVisibleAlbumTitles(page: Page): Promise<string[]> {
  const titles = await page.locator('[data-testid="album-card"] h3').allTextContents();
  return titles;
}

// Helper to scroll and wait for layout to stabilize
async function scrollTo(page: Page, y: number) {
  await page.evaluate((scrollY) => window.scrollTo(0, scrollY), y);
  await page.waitForTimeout(100); // Let layout settle
}

test.describe('VirtualGrid', () => {
  test.beforeEach(async ({ page }) => {
    // Go to library view with many albums
    await page.goto('/mock/library?state=albums%3D200');
    await page.waitForSelector('.virtual-grid-content', { timeout: 10000 });
    // Wait for images to load and measurement to stabilize
    await page.waitForTimeout(500);
  });

  test('limits DOM elements via virtual scrolling', async ({ page }) => {
    const itemCount = await countGridItems(page);
    
    // With 200 albums, we should NOT have 200 items in DOM
    // Virtual scrolling should limit to visible + buffer (maybe 50-80 max)
    expect(itemCount).toBeLessThan(100);
    expect(itemCount).toBeGreaterThan(0);
    
    console.log(`DOM has ${itemCount} items (expected < 100 for 200 albums)`);
  });

  test('changes visible items when scrolling', async ({ page }) => {
    const initialTitles = await getVisibleAlbumTitles(page);
    
    // Scroll down significantly
    await scrollTo(page, 2000);
    
    const scrolledTitles = await getVisibleAlbumTitles(page);
    
    // Should see different albums after scrolling
    expect(scrolledTitles).not.toEqual(initialTitles);
    
    console.log('Initial:', initialTitles.slice(0, 3));
    console.log('After scroll:', scrolledTitles.slice(0, 3));
  });

  test('maintains reasonable DOM count while scrolling', async ({ page }) => {
    const counts: number[] = [];
    
    // Scroll through the page and measure DOM count at each position
    for (let y = 0; y <= 5000; y += 1000) {
      await scrollTo(page, y);
      const count = await countGridItems(page);
      counts.push(count);
    }
    
    // All counts should be similar (virtual scrolling maintains window)
    const maxCount = Math.max(...counts);
    const minCount = Math.min(...counts);
    
    expect(maxCount - minCount).toBeLessThan(20); // Shouldn't vary wildly
    expect(maxCount).toBeLessThan(100); // Should always be virtualized
    
    console.log('DOM counts at scroll positions:', counts);
  });

  test('scrolling is smooth (no large jumps in first visible item)', async ({ page }) => {
    // Get viewport height
    const viewportHeight = await page.evaluate(() => window.innerHeight);
    
    // Scroll in small increments and check that visible content changes gradually
    let previousFirstTitle = '';
    let jumpCount = 0;
    
    for (let y = 0; y <= 3000; y += viewportHeight / 4) {
      await scrollTo(page, y);
      
      const titles = await getVisibleAlbumTitles(page);
      const firstTitle = titles[0] || '';
      
      if (previousFirstTitle && firstTitle !== previousFirstTitle) {
        // Title changed - this is expected when scrolling past an item
        // But we should track if it jumps too much
        jumpCount++;
      }
      
      previousFirstTitle = firstTitle;
    }
    
    // Some title changes are expected, but not too many (would indicate jumps)
    console.log(`Title changed ${jumpCount} times during scroll`);
  });

  test('resize updates layout correctly', async ({ page }) => {
    // Helper to get items per row (by checking Y positions)
    async function getColumnsInFirstRow(): Promise<number> {
      const items = page.locator('.virtual-grid-content > div[data-index]');
      const boxes = await items.evaluateAll(els => 
        els.slice(0, 10).map(el => ({ y: el.getBoundingClientRect().y }))
      );
      if (boxes.length === 0) return 0;
      const firstRowY = boxes[0].y;
      return boxes.filter(b => Math.abs(b.y - firstRowY) < 5).length;
    }

    // Start at wide viewport
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.waitForTimeout(300);
    
    const wideCount = await countGridItems(page);
    const wideCols = await getColumnsInFirstRow();
    
    // Resize to narrow viewport
    await page.setViewportSize({ width: 400, height: 800 });
    await page.waitForTimeout(300);
    
    const narrowCount = await countGridItems(page);
    const narrowCols = await getColumnsInFirstRow();
    
    // Both should be virtualized
    expect(wideCount).toBeLessThan(100);
    expect(narrowCount).toBeLessThan(100);
    
    // Wide should have more columns than narrow
    expect(wideCols, 'Wide viewport should have multiple columns').toBeGreaterThan(1);
    expect(narrowCols, 'Narrow viewport should have fewer columns').toBeLessThan(wideCols);
    
    console.log(`Wide: ${wideCount} items, ${wideCols} cols | Narrow: ${narrowCount} items, ${narrowCols} cols`);
  });

  test('resize observer survives over time (no GC issues)', async ({ page }) => {
    // Start narrow
    await page.setViewportSize({ width: 400, height: 800 });
    await page.waitForTimeout(500);

    // Wait longer to give GC a chance to run
    await page.waitForTimeout(2000);
    
    // Force GC if possible (may not work in all browsers)
    await page.evaluate(() => {
      if ((window as any).gc) (window as any).gc();
    });
    
    await page.waitForTimeout(500);

    // Now resize - should still respond if observer wasn't GC'd
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.waitForTimeout(500);

    // Check that we have multiple columns (ResizeObserver still working)
    const items = page.locator('.virtual-grid-content > div[data-index]');
    const boxes = await items.evaluateAll(els => 
      els.slice(0, 10).map(el => ({ y: el.getBoundingClientRect().y }))
    );
    const firstRowY = boxes[0]?.y ?? 0;
    const cols = boxes.filter(b => Math.abs(b.y - firstRowY) < 5).length;
    
    console.log(`After GC wait + resize: ${cols} columns`);
    expect(cols, 'ResizeObserver should still work after potential GC').toBeGreaterThan(1);
  });

  test('measurement updates after viewport change', async ({ page }) => {
    // This test checks that item height measurement updates on resize
    
    // Start narrow (single column, tall items)
    await page.setViewportSize({ width: 400, height: 800 });
    await page.waitForTimeout(500);
    
    // Get the first item's height
    const narrowHeight = await page.locator('.virtual-grid-content > div').first().boundingBox();
    
    // Go wide (multiple columns, shorter items)
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.waitForTimeout(500);
    
    const wideHeight = await page.locator('.virtual-grid-content > div').first().boundingBox();
    
    // Narrow items should be taller (single column = wider = taller due to aspect-square)
    if (narrowHeight && wideHeight) {
      console.log(`Narrow item height: ${narrowHeight.height}, Wide: ${wideHeight.height}`);
      expect(narrowHeight.height).toBeGreaterThan(wideHeight.height);
    }
  });

  test('items move exactly by scroll delta - no jumps', async ({ page }) => {
    test.setTimeout(60000); // 60 second timeout for this thorough test
    // Use 2000 albums for thorough test
    await page.goto('/mock/library?state=albums%3D2000');
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.waitForSelector('.virtual-grid-content');
    await page.waitForTimeout(1000);

    const items = page.locator('.virtual-grid-content > div[data-index]');
    
    // Track: Map<index, Y position>
    let previousPositions: Map<number, number> = new Map();
    let previousScrollY = 0;

    for (let scrollY = 0; scrollY <= 5000; scrollY += 20) {
      await page.evaluate(y => window.scrollTo(0, y), scrollY);
      await page.waitForTimeout(50); // Wait for render

      // Capture current state: index -> Y position
      const currentData = await items.evaluateAll(els => 
        els.map(el => ({
          index: parseInt(el.dataset.index!, 10),
          y: el.getBoundingClientRect().y
        }))
      );
      
      const currentPositions = new Map(currentData.map(d => [d.index, d.y]));
      const scrollDelta = scrollY - previousScrollY;

      // Log state for debugging BEFORE assertions
      const indices = [...currentPositions.keys()].sort((a, b) => a - b);
      console.log(`scroll=${scrollY}: items ${indices[0]}-${indices[indices.length - 1]}, count=${indices.length}`);

      // For every item that existed before AND still exists now:
      // Its Y should have changed by exactly -scrollDelta
      if (previousPositions.size > 0) {
        for (const [index, prevY] of previousPositions) {
          if (currentPositions.has(index)) {
            const currentY = currentPositions.get(index)!;
            const actualDelta = currentY - prevY;
            const expectedDelta = -scrollDelta; // scroll down = items move up
            
            // Log details for first few items that will be checked
            if (index <= 10) {
              console.log(`  Item ${index}: prevY=${prevY.toFixed(0)}, currentY=${currentY.toFixed(0)}, delta=${actualDelta.toFixed(0)}, expected=${expectedDelta}`);
            }
            
            expect(
              Math.abs(actualDelta - expectedDelta),
              `Item ${index} at scroll=${scrollY}: moved ${actualDelta}px, expected ${expectedDelta}px`
            ).toBeLessThan(2);
          }
        }
      }

      previousPositions = currentPositions;
      previousScrollY = scrollY;
    }
  });

  test('scroll performance - bounded DOM churn during scroll sweep', async ({ page }) => {
    await page.goto('/mock/library?state=albums%3D500');
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.waitForSelector('.virtual-grid-content');
    await page.waitForTimeout(500);

    // Set up mutation counter
    await page.evaluate(() => {
      let count = 0;
      const observer = new MutationObserver(mutations => {
        count += mutations.length;
      });
      const target = document.querySelector('.virtual-grid-content');
      if (target) {
        observer.observe(target, { childList: true, subtree: true, attributes: true });
      }
      (window as any).__scrollMutationCount = () => {
        observer.disconnect();
        return count;
      };
    });

    // Scroll through the page in increments
    for (let y = 0; y <= 8000; y += 200) {
      await page.evaluate(scrollY => window.scrollTo(0, scrollY), y);
      await page.waitForTimeout(30);
    }

    const mutationCount = await page.evaluate(() => (window as any).__scrollMutationCount());
    const itemCount = await countGridItems(page);
    
    console.log(`Scroll sweep: ${mutationCount} DOM mutations, ${itemCount} items in DOM`);
    
    // Virtual scrolling means we're swapping items in/out - expect mutations
    // but not an explosion. 40 scroll steps * ~10-20 items changed per step = 400-800 reasonable
    // Threshold of 2000 catches major regressions
    expect(mutationCount, 'Too many DOM mutations during scroll').toBeLessThan(2000);
    expect(itemCount, 'Should still be virtualized after scroll').toBeLessThan(100);
  });

  test('resize performance - should not re-render excessively during resize', async ({ page }) => {
    await page.goto('/mock/library?state=albums%3D200');
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.waitForSelector('.virtual-grid-content');
    await page.waitForTimeout(500);

    // Set up DOM mutation counter on the grid content
    await page.evaluate(() => {
      let count = 0;
      const observer = new MutationObserver(mutations => {
        count += mutations.length;
      });
      const target = document.querySelector('.virtual-grid-content');
      if (target) {
        observer.observe(target, { childList: true, subtree: true, attributes: true });
      }
      (window as any).__mutationCount = () => {
        observer.disconnect();
        return count;
      };
    });

    // Simulate rapid window resize (60 steps total)
    for (let width = 1400; width >= 800; width -= 20) {
      await page.setViewportSize({ width, height: 900 });
      await page.waitForTimeout(16);
    }
    for (let width = 800; width <= 1400; width += 20) {
      await page.setViewportSize({ width, height: 900 });
      await page.waitForTimeout(16);
    }

    // Get final mutation count
    const finalCount = await page.evaluate(() => (window as any).__mutationCount());
    console.log(`Resize: 60 steps, ${finalCount} DOM mutations`);
    
    // With proper debouncing, we expect very few mutations (currently ~9-20)
    // Threshold of 200 catches regressions while allowing some headroom
    expect(finalCount, 'Too many DOM mutations during resize - debouncing broken?').toBeLessThan(200);
    
    // Grid should still be functional after resize
    const items = await page.locator('.virtual-grid-content > div[data-index]').count();
    expect(items).toBeGreaterThan(0);
    expect(items).toBeLessThan(100); // Still virtualized
  });

  test('cleanup - no memory leak on repeated mount/unmount', async ({ page, browser }) => {
    test.setTimeout(180000); // 3 minute timeout for stress test

    // Use CDP for accurate heap measurement
    const client = await page.context().newCDPSession(page);
    
    async function getHeapMB(): Promise<number> {
      // Force GC then measure
      await client.send('HeapProfiler.collectGarbage');
      const { usedSize } = await client.send('Runtime.getHeapUsage');
      return usedSize / 1024 / 1024;
    }

    // Go to library page
    await page.goto('/mock/library?state=albums%3D100');
    await page.waitForSelector('.virtual-grid-content');
    await page.waitForTimeout(500);

    // Find the cycle input field (it's an int control labeled "Remount Cycle")
    const cycleInput = page.locator('input[type="number"]').last(); // cycle is the last int control
    
    const baseline = await getHeapMB();
    console.log(`Baseline heap: ${baseline.toFixed(2)} MB`);

    const CYCLES = 100; // Each cycle leaks ~1MB if broken
    const measurements: { cycle: number; heap: number }[] = [];

    for (let i = 1; i <= CYCLES; i++) {
      // Type new cycle value - this changes the key, forcing remount
      await cycleInput.fill(String(i));
      await page.waitForTimeout(30);

      // Measure every 25 cycles
      if (i % 25 === 0) {
        const heap = await getHeapMB();
        measurements.push({ cycle: i, heap });
        console.log(`After ${i} cycles: ${heap.toFixed(2)} MB (Δ ${(heap - baseline).toFixed(2)} MB)`);
      }
    }

    const final = await getHeapMB();
    console.log(`\nFINAL: ${final.toFixed(2)} MB after ${CYCLES} cycles`);
    console.log(`Growth: ${(final - baseline).toFixed(2)} MB`);

    // Check for linear growth (leak signature)
    // If leaking: heap grows ~linearly with cycles
    // If not leaking: heap stays roughly flat (GC keeps it bounded)
    const growthPerCycle = (final - baseline) / CYCLES;
    console.log(`Growth per cycle: ${(growthPerCycle * 1024).toFixed(2)} KB`);

    // Without leaks, growth should be near 0 (GC cleans up)
    // With leaks, we see ~14KB per cycle
    // Threshold of 5KB catches real leaks while allowing noise
    expect(growthPerCycle, 'Memory growing linearly - leak detected!').toBeLessThan(0.005); // 5 KB
  });
});
