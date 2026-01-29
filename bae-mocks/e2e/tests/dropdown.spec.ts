import { test, expect, Page } from '@playwright/test';

// Wait for all images in the viewport to finish loading
async function waitForImages(page: Page, selector: string, timeout = 15000): Promise<void> {
  await page.waitForFunction(
    (sel) => {
      const images = document.querySelectorAll(`${sel} img`);
      if (images.length === 0) return true;
      return Array.from(images).every((img) => {
        const imgEl = img as HTMLImageElement;
        return imgEl.complete && imgEl.naturalHeight > 0;
      });
    },
    selector,
    { timeout }
  );
}

test.describe('Dropdown Component', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/dropdown-test');
    // Wait for the grid to render (longer timeout for CI WASM builds)
    await page.waitForSelector('[data-testid="album-card"]', { timeout: 30000 });
    // Wait for album cover images to load
    await waitForImages(page, '[data-testid="album-card"]');
  });

  test('FloatingUIDOM is available', async ({ page }) => {
    const hasFloatingUI = await page.evaluate(() => {
      return typeof (window as any).FloatingUIDOM !== 'undefined';
    });
    console.log('FloatingUIDOM available:', hasFloatingUI);
    
    if (hasFloatingUI) {
      const methods = await page.evaluate(() => {
        const fui = (window as any).FloatingUIDOM;
        return Object.keys(fui);
      });
      console.log('FloatingUIDOM methods:', methods);
    }
    
    expect(hasFloatingUI, 'FloatingUIDOM should be available').toBe(true);
  });

  async function getDropdownToggle(page: Page, albumId: string) {
    return page.locator(`#album-card-btn-${albumId}`);
  }

  async function getOpenPopover(page: Page) {
    // Popover API adds :popover-open pseudo-class when visible
    return page.locator('[popover]:popover-open');
  }

  async function isDropdownOpen(page: Page): Promise<boolean> {
    const popover = await getOpenPopover(page);
    return await popover.count() > 0;
  }

  async function getDropdownPosition(page: Page): Promise<{ x: number; y: number } | null> {
    const popover = await getOpenPopover(page);
    if (await popover.count() === 0) return null;
    const box = await popover.boundingBox();
    return box ? { x: box.x, y: box.y } : null;
  }

  // Wait for dropdown to become visible (opacity: 1)
  // The dropdown starts with opacity: 0 and only becomes visible after positioning completes
  async function waitForDropdownVisible(page: Page, timeout = 15000): Promise<{ x: number; y: number }> {
    const popover = await getOpenPopover(page);
    
    // Wait for opacity to become 1 (positioning sets opacity: 1 after computing position)
    await expect(popover).toHaveCSS('opacity', '1', { timeout });
    
    const box = await popover.boundingBox();
    return box ? { x: box.x, y: box.y } : { x: 0, y: 0 };
  }

  async function getTogglePosition(page: Page, albumId: string): Promise<{ x: number; y: number }> {
    const toggle = await getDropdownToggle(page, albumId);
    const box = await toggle.boundingBox();
    if (!box) throw new Error(`Toggle for ${albumId} not found`);
    return { x: box.x, y: box.y };
  }

  test('dropdown opens when clicking toggle', async ({ page }) => {
    const toggle = await getDropdownToggle(page, 'album-1');
    
    // Hover to make the toggle visible
    const albumCard = page.locator('[data-testid="album-card"]').first();
    await albumCard.hover();
    await page.waitForTimeout(100);
    
    // Click toggle
    await toggle.click();
    await page.waitForTimeout(200);
    
    // Dropdown should be open
    expect(await isDropdownOpen(page)).toBe(true);
  });

  test('dropdown opens near the trigger, not at (0,0)', async ({ page }) => {
    const albumCard = page.locator('[data-testid="album-card"]').first();
    await albumCard.hover();
    await page.waitForTimeout(100);
    
    const toggle = await getDropdownToggle(page, 'album-1');
    const togglePos = await getTogglePosition(page, 'album-1');
    
    await toggle.click();

    const dropdownPos = await waitForDropdownVisible(page);

    // Dropdown should be within 200px of the toggle (accounting for offset and placement)
    const distance = Math.sqrt(
      Math.pow(dropdownPos.x - togglePos.x, 2) +
      Math.pow(dropdownPos.y - togglePos.y, 2)
    );

    console.log(`Toggle at (${togglePos.x}, ${togglePos.y}), Dropdown at (${dropdownPos.x}, ${dropdownPos.y}), Distance: ${distance}`);

    expect(distance, 'Dropdown should be near the toggle').toBeLessThan(200);
    expect(dropdownPos.x, 'Dropdown X should not be at 0').toBeGreaterThan(10);
    expect(dropdownPos.y, 'Dropdown Y should not be at 0').toBeGreaterThan(10);
  });

  test('clicking outside closes the dropdown (light dismiss)', async ({ page }) => {
    const albumCard = page.locator('[data-testid="album-card"]').first();
    await albumCard.hover();
    await page.waitForTimeout(100);
    
    const toggle = await getDropdownToggle(page, 'album-1');
    await toggle.click();
    await page.waitForTimeout(200);
    
    expect(await isDropdownOpen(page)).toBe(true);
    
    // Click outside (on body)
    await page.click('body', { position: { x: 10, y: 10 } });
    await page.waitForTimeout(200);
    
    expect(await isDropdownOpen(page)).toBe(false);
  });

  test('clicking toggle again closes the dropdown', async ({ page }) => {
    const albumCard = page.locator('[data-testid="album-card"]').first();
    await albumCard.hover();
    await page.waitForTimeout(100);
    
    const toggle = await getDropdownToggle(page, 'album-1');
    
    // Open
    await toggle.click();
    await page.waitForTimeout(200);
    expect(await isDropdownOpen(page)).toBe(true);
    
    // Close by clicking toggle again
    await toggle.click();
    await page.waitForTimeout(200);
    expect(await isDropdownOpen(page)).toBe(false);
  });

  test('after light dismiss, same toggle reopens immediately (no double-click needed)', async ({ page }) => {
    test.setTimeout(120000); // 2 minute timeout for this slow test
    const SLOW = 3000; // 3 seconds between actions for visibility
    
    // Inject a visible cursor indicator
    await page.addStyleTag({
      content: `
        #playwright-cursor {
          position: fixed;
          width: 20px;
          height: 20px;
          background: red;
          border-radius: 50%;
          pointer-events: none;
          z-index: 999999;
          transform: translate(-50%, -50%);
          box-shadow: 0 0 10px rgba(255,0,0,0.5);
        }
      `
    });
    await page.addScriptTag({
      content: `
        const cursor = document.createElement('div');
        cursor.id = 'playwright-cursor';
        document.body.appendChild(cursor);
        document.addEventListener('mousemove', (e) => {
          cursor.style.left = e.clientX + 'px';
          cursor.style.top = e.clientY + 'px';
        });
      `
    });
    
    // Helper to smoothly move mouse to element center
    async function smoothMoveTo(x: number, y: number) {
      await page.mouse.move(x, y, { steps: 25 }); // 25 intermediate steps
    }
    
    async function smoothHover(locator: any) {
      const box = await locator.boundingBox();
      if (box) {
        await smoothMoveTo(box.x + box.width / 2, box.y + box.height / 2);
      }
    }
    
    async function smoothClick(locator: any) {
      await smoothHover(locator);
      await page.waitForTimeout(800); // Pause over element before clicking
      await page.mouse.down();
      await page.waitForTimeout(50);
      await page.mouse.up();
    }
    
    // Wait for user to find the window
    console.log('>>> FIND THE BROWSER WINDOW - look for the RED DOT cursor - test starts in 5 seconds...');
    await page.waitForTimeout(5000);
    
    const card1 = page.locator('[data-testid="album-card"]').nth(0);
    const toggle = await getDropdownToggle(page, 'album-1');
    
    // Warm up - toggle the dropdown several times first
    console.log('Warming up - toggling dropdown open/close several times...');
    for (let i = 1; i <= 3; i++) {
      console.log(`  Toggle cycle ${i}: opening...`);
      await smoothHover(card1);
      await page.waitForTimeout(1000);
      await smoothClick(toggle);
      await page.waitForTimeout(1500);
      
      console.log(`  Toggle cycle ${i}: closing via toggle click...`);
      await smoothClick(toggle);
      await page.waitForTimeout(1500);
    }
    
    // Move away then back
    console.log('Moving mouse away...');
    await smoothMoveTo(400, 400);
    await page.waitForTimeout(1000);
    
    const albumCard = card1;
    
    // Helper to check if the toggle button is visible (opacity > 0)
    async function isToggleVisible(): Promise<boolean> {
      const opacity = await toggle.evaluate((el) => {
        return window.getComputedStyle(el).opacity;
      });
      return parseFloat(opacity) > 0.5;
    }
    
    // Helper to check if hover overlay is active (has bg-black/40 background)
    async function isHoverOverlayActive(): Promise<boolean> {
      const overlay = albumCard.locator('.absolute.inset-0').first();
      const bg = await overlay.evaluate((el) => {
        return window.getComputedStyle(el).backgroundColor;
      });
      // bg-black/40 = rgba(0, 0, 0, 0.4)
      return bg.includes('0.4') || bg.includes('0, 0, 0');
    }
    
    console.log('Step 1: Hovering album card...');
    await smoothHover(albumCard);
    await page.waitForTimeout(SLOW);
    console.log('  -> Toggle visible?', await isToggleVisible());
    console.log('  -> Hover overlay active?', await isHoverOverlayActive());
    
    // Get toggle position while it's visible
    const toggleBox = await toggle.boundingBox();
    expect(toggleBox).not.toBeNull();
    const toggleCenter = {
      x: toggleBox!.x + toggleBox!.width / 2,
      y: toggleBox!.y + toggleBox!.height / 2,
    };
    
    // Open
    console.log('Step 2: Clicking toggle to OPEN dropdown...');
    await smoothClick(toggle);
    await page.waitForTimeout(SLOW);
    const openAfterFirstClick = await isDropdownOpen(page);
    console.log('  -> Dropdown open?', openAfterFirstClick);
    console.log('  -> Toggle visible?', await isToggleVisible());
    console.log('  -> Hover overlay active?', await isHoverOverlayActive());
    expect(openAfterFirstClick).toBe(true);
    
    // Light dismiss by clicking ON the album card image (outside the toggle/dropdown)
    // This is more realistic - user clicks on the album art, not far away
    const cardBox = await albumCard.boundingBox();
    const clickOnCardPos = {
      x: cardBox!.x + 50,  // Left side of card
      y: cardBox!.y + 100, // Middle of card image
    };
    console.log('Step 3: Clicking ON album card to DISMISS (light dismiss)...', clickOnCardPos);
    await smoothMoveTo(clickOnCardPos.x, clickOnCardPos.y);
    await page.mouse.down();
    await page.waitForTimeout(50);
    await page.mouse.up();
    await page.waitForTimeout(SLOW);
    const openAfterDismiss = await isDropdownOpen(page);
    console.log('  -> Dropdown open?', openAfterDismiss);
    console.log('  -> Toggle visible? (mouse still on card)', await isToggleVisible());
    console.log('  -> Hover overlay active? (mouse still on card)', await isHoverOverlayActive());
    expect(openAfterDismiss, 'Dropdown should be closed after light dismiss').toBe(false);
    
    // NOW move mouse off the card, then check state
    console.log('Step 3b: Moving mouse OFF the card...');
    await smoothMoveTo(10, 10);
    await page.waitForTimeout(SLOW);
    console.log('  -> Toggle visible? (mouse off card)', await isToggleVisible());
    console.log('  -> Hover overlay active? (mouse off card)', await isHoverOverlayActive());
    
    // After light dismiss AND mouse moved away, overlay should NOT be active
    const overlayStillActive = await isHoverOverlayActive();
    console.log('  -> BUG CHECK: Overlay still active after dismiss?', overlayStillActive);
    
    // DO NOT hover - click directly where the toggle was
    console.log('Step 4: Clicking at toggle position to REOPEN (single click)...', toggleCenter);
    await smoothMoveTo(toggleCenter.x, toggleCenter.y);
    await page.waitForTimeout(1500); // Wait over toggle position before clicking
    await page.mouse.down();
    await page.waitForTimeout(50);
    await page.mouse.up();
    await page.waitForTimeout(SLOW);
    
    const openAfterReclick = await isDropdownOpen(page);
    console.log('  -> Dropdown open?', openAfterReclick);
    
    // If not open after first click, try a second click (this would confirm the double-click bug)
    if (!openAfterReclick) {
      console.log('Step 5: First click did NOT open - trying SECOND click...');
      await page.mouse.down();
      await page.waitForTimeout(50);
      await page.mouse.up();
      await page.waitForTimeout(SLOW);
      const openAfterSecondClick = await isDropdownOpen(page);
      console.log('  -> Dropdown open after second click?', openAfterSecondClick);
    }
    
    // Should reopen with a single click
    expect(openAfterReclick, 'Dropdown should reopen with single click after light dismiss (no hover first)').toBe(true);
    
    // The hover overlay should have been deactivated after light dismiss
    expect(overlayStillActive, 'Hover overlay should deactivate after light dismiss (show_dropdown should be false)').toBe(false);
  });

  test('opening different dropdowns positions correctly each time', async ({ page }) => {
    // Open first album dropdown
    const card1 = page.locator('[data-testid="album-card"]').nth(0);
    await card1.hover();
    await page.waitForTimeout(100);
    
    const toggle1 = await getDropdownToggle(page, 'album-1');
    const toggle1Pos = await getTogglePosition(page, 'album-1');
    
    await toggle1.click();
    // Wait for dropdown to become visible (opacity changes to 1 only after positioning)
    const dropdown1Pos = await waitForDropdownVisible(page);
    console.log(`Album 1: Toggle at (${toggle1Pos.x}, ${toggle1Pos.y}), Dropdown at (${dropdown1Pos.x}, ${dropdown1Pos.y})`);
    
    // Close it
    await toggle1.click();
    await page.waitForTimeout(200);
    
    // Open third album dropdown (different position in grid)
    const card3 = page.locator('[data-testid="album-card"]').nth(2);
    await card3.hover();
    await page.waitForTimeout(100);
    
    const toggle3 = await getDropdownToggle(page, 'album-3');
    const toggle3Pos = await getTogglePosition(page, 'album-3');
    
    await toggle3.click();
    // Wait for dropdown to become visible (opacity changes to 1 only after positioning)
    const dropdown3Pos = await waitForDropdownVisible(page);
    console.log(`Album 3: Toggle at (${toggle3Pos.x}, ${toggle3Pos.y}), Dropdown at (${dropdown3Pos.x}, ${dropdown3Pos.y})`);
    
    // The dropdown for album 3 should be near album 3's toggle, NOT album 1's
    const distanceToToggle3 = Math.sqrt(
      Math.pow(dropdown3Pos.x - toggle3Pos.x, 2) + 
      Math.pow(dropdown3Pos.y - toggle3Pos.y, 2)
    );
    
    const distanceToToggle1 = Math.sqrt(
      Math.pow(dropdown3Pos.x - toggle1Pos.x, 2) + 
      Math.pow(dropdown3Pos.y - toggle1Pos.y, 2)
    );
    
    expect(distanceToToggle3, 'Dropdown should be near album 3 toggle').toBeLessThan(200);
    
    // If toggle1 and toggle3 are far apart, dropdown should be closer to toggle3
    if (Math.abs(toggle3Pos.x - toggle1Pos.x) > 100 || Math.abs(toggle3Pos.y - toggle1Pos.y) > 100) {
      expect(distanceToToggle3, 'Dropdown should be closer to album 3 toggle than album 1').toBeLessThan(distanceToToggle1);
    }
  });

  test('dropdown menu items are visible and clickable', async ({ page }) => {
    const albumCard = page.locator('[data-testid="album-card"]').first();
    await albumCard.hover();
    await page.waitForTimeout(100);
    
    const toggle = await getDropdownToggle(page, 'album-1');
    await toggle.click();
    await page.waitForTimeout(200);
    
    // Check menu items are visible
    const playButton = page.locator('[popover]:popover-open button:has-text("Play")');
    const addToQueueButton = page.locator('[popover]:popover-open button:has-text("Add to Queue")');
    
    await expect(playButton).toBeVisible();
    await expect(addToQueueButton).toBeVisible();
    
    // Click play - should close dropdown
    await playButton.click();
    await page.waitForTimeout(200);
    
    expect(await isDropdownOpen(page)).toBe(false);
  });

  test('hover overlay stays visible while dropdown is open', async ({ page }) => {
    const albumCard = page.locator('[data-testid="album-card"]').first();
    await albumCard.hover();
    await page.waitForTimeout(100);
    
    const toggle = await getDropdownToggle(page, 'album-1');
    await toggle.click();
    await page.waitForTimeout(200);
    
    // Move mouse away from the card
    await page.mouse.move(10, 10);
    await page.waitForTimeout(100);
    
    // Toggle button should still be visible (not hidden due to hover state)
    await expect(toggle).toBeVisible();
    
    // Dropdown should still be open
    expect(await isDropdownOpen(page)).toBe(true);
  });
});
