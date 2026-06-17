import type { APIRoute } from 'astro';
import { getLatest } from '../lib/releases';

// Published at /latest.json: the newest version + permanent download URL per
// platform/edition. Regenerated on each website build (every app release
// triggers a redeploy), so it always names the current releases. A machine
// can read this to find the latest build without scraping the releases page.
export const prerender = true;

export const GET: APIRoute = async () => {
  return new Response(JSON.stringify(await getLatest(), null, 2), {
    headers: { 'content-type': 'application/json' },
  });
};
