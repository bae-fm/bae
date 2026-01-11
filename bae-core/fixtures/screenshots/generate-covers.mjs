import sharp from 'sharp';
import { readFileSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Curated color palette - warm, muted tones
const colors = [
  '#8B5CF6', // Purple
  '#EC4899', // Pink
  '#F59E0B', // Amber
  '#10B981', // Emerald
  '#3B82F6', // Blue
  '#EF4444', // Red
  '#6366F1', // Indigo
  '#14B8A6', // Teal
  '#F97316', // Orange
  '#84CC16', // Lime
  '#A855F7', // Violet
  '#06B6D4', // Cyan
  '#D946EF', // Fuchsia
  '#22C55E', // Green
  '#0EA5E9', // Sky
  '#E11D48', // Rose
  '#7C3AED', // Purple dark
  '#0D9488', // Teal dark
  '#CA8A04', // Yellow dark
  '#DC2626', // Red dark
];

const data = JSON.parse(readFileSync(join(__dirname, 'data.json'), 'utf-8'));

mkdirSync(join(__dirname, 'covers'), { recursive: true });

for (let i = 0; i < data.albums.length; i++) {
  const album = data.albums[i];
  const color = colors[i % colors.length];
  const filename = `${album.artist.toLowerCase().replace(/\s+/g, '-')}_${album.title.toLowerCase().replace(/\s+/g, '-')}.png`;
  
  // Create a 500x500 solid color image
  const svg = `
    <svg width="500" height="500" xmlns="http://www.w3.org/2000/svg">
      <rect width="500" height="500" fill="${color}"/>
    </svg>
  `;
  
  await sharp(Buffer.from(svg))
    .png()
    .toFile(join(__dirname, 'covers', filename));
  
  console.log(`Generated: ${filename}`);
}

console.log(`\nGenerated ${data.albums.length} cover images`);

