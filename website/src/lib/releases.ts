/**
 * Latest published download per platform/edition, resolved from the GitHub
 * Releases API at build time. The site links to permanent, versioned asset URLs
 * (e.g. .../releases/download/android-v0.3/bae-android.apk) rather than a
 * rolling `-latest` release: the repo's releases are immutable, so a tag can't
 * be reused and an asset can't be replaced in place — the old "clobber the
 * `-latest` asset each release" pattern can't work. Each app release triggers a
 * website redeploy, which reruns this and re-bakes the download buttons and
 * `/latest.json` against the newest versioned release.
 */

const REPO = 'bae-fm/bae';

export type Platform = 'macos' | 'ios' | 'android' | 'windows';
export type Edition = 'full' | 'baeium';

/** The release asset filename per platform/edition, fixed by each build. */
const ASSETS: Record<Platform, Record<Edition, string>> = {
  macos: { full: 'bae-macos.dmg', baeium: 'baeium-macos.dmg' },
  ios: { full: 'bae-ios.ipa', baeium: 'baeium-ios.ipa' },
  android: { full: 'bae-android.apk', baeium: 'baeium-android.apk' },
  windows: { full: 'bae-windows-x64-setup.exe', baeium: 'baeium-windows-x64-setup.exe' },
};

export interface EditionDownload {
  version: string;
  url: string;
}
/** A platform's available editions; an edition with no asset yet is absent. */
export type PlatformDownloads = Partial<Record<Edition, EditionDownload>>;
/** Latest per platform; a platform with no release yet is absent. */
export type LatestManifest = Partial<Record<Platform, PlatformDownloads>>;

interface GhAsset {
  name: string;
  browser_download_url: string;
}
interface GhRelease {
  tag_name: string;
  draft: boolean;
  assets: GhAsset[];
}

/**
 * Compare dotted numeric versions so "0.10" sorts above "0.2". A shorter version
 * is normalized by padding absent trailing segments with zero — "0.3" equals
 * "0.3.0" — which is the standard semver convention, made explicit here rather
 * than defaulted inline.
 */
function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  const len = Math.max(pa.length, pb.length);
  while (pa.length < len) pa.push(0);
  while (pb.length < len) pb.push(0);
  for (let i = 0; i < len; i++) {
    if (pa[i] !== pb[i]) return pa[i] > pb[i] ? 1 : -1;
  }
  return 0;
}

async function fetchReleases(): Promise<GhRelease[]> {
  const headers: Record<string, string> = {
    // GitHub rejects API requests with no User-Agent.
    'User-Agent': 'bae-website-build',
    Accept: 'application/vnd.github+json',
  };
  // Authenticated when a token is in the build env (GitHub Actions runner):
  // lifts the 60/hr unauthenticated cap. Anonymous (e.g. a Vercel build) still
  // works — this is one request per build.
  const token = process.env.GITHUB_TOKEN;
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(`https://api.github.com/repos/${REPO}/releases?per_page=100`, { headers });
  if (!res.ok) {
    throw new Error(
      `GitHub releases API returned ${res.status} ${res.statusText}; cannot resolve download links`,
    );
  }
  return (await res.json()) as GhRelease[];
}

function buildManifest(releases: GhRelease[]): LatestManifest {
  const manifest: LatestManifest = {};
  for (const platform of Object.keys(ASSETS) as Platform[]) {
    const prefix = `${platform}-v`;
    const latest = releases
      .filter((r) => !r.draft && r.tag_name.startsWith(prefix))
      .map((r) => ({ release: r, version: r.tag_name.slice(prefix.length) }))
      .sort((a, b) => compareVersions(b.version, a.version))[0];
    if (!latest) {
      // Expected for a platform that hasn't shipped (iOS, Windows): it's simply
      // omitted, and its button renders "coming soon". Logged so the build shows
      // which platforms resolved and which didn't.
      console.info(`releases: no published ${platform}-v* release yet; omitting from manifest`);
      continue;
    }
    const downloads: PlatformDownloads = {};
    for (const edition of ['full', 'baeium'] as Edition[]) {
      const asset = latest.release.assets.find((a) => a.name === ASSETS[platform][edition]);
      if (asset) {
        downloads[edition] = { version: latest.version, url: asset.browser_download_url };
      } else {
        // The release exists but is missing this edition's asset. Anomalous for a
        // full build; a known gap for baeium on macOS/iOS, whose serialized
        // per-edition release matrix can't append the second edition to an
        // immutable release. Either way, name what's missing rather than drop it
        // silently.
        console.warn(
          `releases: ${platform}-v${latest.version} has no ${ASSETS[platform][edition]} asset; omitting ${platform}/${edition}`,
        );
      }
    }
    if (Object.keys(downloads).length > 0) manifest[platform] = downloads;
  }
  return manifest;
}

let cache: Promise<LatestManifest> | undefined;

/** Build-time-memoized: one releases fetch per build, shared by every caller. */
export function getLatest(): Promise<LatestManifest> {
  if (!cache) cache = fetchReleases().then(buildManifest);
  return cache;
}
