import { test, Page } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';

// Only run screenshot generation in CI
test.skip(() => !process.env.CI, 'Screenshots only run in CI');

const OUTPUT_DIR = path.join(__dirname, '../../website/public/screenshots');

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

test.beforeAll(async () => {
  // Ensure output directory exists
  fs.mkdirSync(OUTPUT_DIR, { recursive: true });
});

test('capture library view', async ({ page }) => {
  await page.goto('/app');
  
  // Wait for the virtual grid AND at least one album card to render
  await page.waitForSelector('.virtual-grid-content [data-testid="album-card"]', { timeout: 30000 });
  
  // Wait for cover images to load
  await waitForImages(page, '.virtual-grid-content');
  
  await page.screenshot({
    path: path.join(OUTPUT_DIR, 'library.png'),
    fullPage: false,
  });
});

test('capture album detail view', async ({ page }) => {
  await page.goto('/app');
  
  // Wait for albums to load
  await page.waitForSelector('[data-testid="album-card"]', { timeout: 30000 });
  
  // Wait for library images to load before clicking
  await waitForImages(page, '.virtual-grid-content');
  
  // Click on the first album card
  const firstAlbumCard = page.locator('[data-testid="album-card"]').first();
  await firstAlbumCard.click();
  
  // Wait for album detail to load
  await page.waitForSelector('[data-testid="album-detail"]', { timeout: 30000 });
  
  // Wait for album detail images to load
  await waitForImages(page, '[data-testid="album-detail"]');
  
  await page.screenshot({
    path: path.join(OUTPUT_DIR, 'album-detail.png'),
    fullPage: false,
  });
});
