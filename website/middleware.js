import { next } from '@vercel/functions';
import { DEFAULT_LOCALE, LOCALES } from './src/lib/locales.mjs';

const LOCALE_COOKIE = 'bae_locale';
const ROUTED_PREFIXES = [
  '/architecture/',
  '/getting-started/',
  '/importing/',
  '/library/',
  '/storage/',
];

const HEADER_TAGS = new Map([
  ['en', ['en']],
  ['es', ['es']],
  ['fr', ['fr']],
  ['de', ['de']],
  ['pt-BR', ['pt', 'pt-br']],
  ['ja', ['ja']],
  ['zh-Hans', ['zh', 'zh-cn', 'zh-sg', 'zh-hans']],
  ['ar', ['ar']],
  ['he', ['he']],
  ['uk', ['uk']],
  ['bg', ['bg']],
  ['pl', ['pl']],
  ['cs', ['cs']],
  ['hr', ['hr']],
]);

const HEADER_LOCALES = LOCALES.map((locale) => ({
  ...locale,
  tags: HEADER_TAGS.get(locale.code) ?? [],
}));

const LOCALE_PATHS = new Set(LOCALES.map((locale) => locale.path).filter(Boolean));

export const config = {
  matcher: [
    '/',
    '/architecture/:path*',
    '/getting-started/:path*',
    '/importing/:path*',
    '/library/:path*',
    '/storage/:path*',
  ],
};

export default function middleware(request) {
  const url = new URL(request.url);
  if (!shouldRoute(url.pathname) || hasLocaleOverride(request.headers.get('cookie'))) {
    return next();
  }

  const localePath = preferredLocalePath(request.headers.get('accept-language'));
  if (!localePath) return next();

  url.pathname = withLocalePrefix(localePath, url.pathname);
  return Response.redirect(url, 307);
}

export function preferredLocalePath(header) {
  for (const preference of parseAcceptLanguage(header)) {
    const locale = localeForTag(preference.tag);
    if (!locale) continue;
    if (locale.code === DEFAULT_LOCALE) return '';
    return `/${locale.path}/`;
  }
  return '';
}

export function parseAcceptLanguage(header) {
  if (!header) return [];
  return header
    .split(',')
    .map((entry, index) => {
      const [rawTag, ...params] = entry.trim().split(';');
      const tag = rawTag.toLowerCase();
      const q = params.reduce((value, param) => {
        const [key, rawValue] = param.trim().split('=');
        if (key !== 'q') return value;
        const parsed = Number(rawValue);
        return Number.isFinite(parsed) ? parsed : value;
      }, 1);
      return { tag, q, index };
    })
    .filter((entry) => entry.tag && entry.q > 0)
    .sort((a, b) => b.q - a.q || a.index - b.index);
}

function shouldRoute(pathname) {
  if (pathname === '/') return true;
  const firstSegment = pathname.split('/')[1];
  if (LOCALE_PATHS.has(firstSegment)) return false;
  return ROUTED_PREFIXES.some((prefix) => pathname.startsWith(prefix));
}

function hasLocaleOverride(cookieHeader) {
  return new RegExp(`(?:^|;\\s*)${LOCALE_COOKIE}=`).test(cookieHeader ?? '');
}

function localeForTag(tag) {
  return HEADER_LOCALES.find((locale) =>
    locale.tags.some((candidate) => languageTagMatches(candidate, tag))
  );
}

function languageTagMatches(candidate, tag) {
  if (candidate === 'zh') return tag === candidate;
  return tag === candidate || tag.startsWith(`${candidate}-`);
}

function withLocalePrefix(localePath, pathname) {
  if (pathname === '/') return localePath;
  return `${localePath.replace(/\/$/, '')}${pathname}`;
}
