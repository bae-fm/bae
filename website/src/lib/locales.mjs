export const DEFAULT_LOCALE = 'en';

export const LOCALES = [
  { code: 'en', path: '', label: 'English', lang: 'en', dir: 'ltr' },
  { code: 'es', path: 'es', label: 'Español', lang: 'es', dir: 'ltr' },
  { code: 'fr', path: 'fr', label: 'Français', lang: 'fr', dir: 'ltr' },
  { code: 'de', path: 'de', label: 'Deutsch', lang: 'de', dir: 'ltr' },
  { code: 'pt-BR', path: 'pt-br', label: 'Português do Brasil', lang: 'pt-BR', dir: 'ltr' },
  { code: 'ja', path: 'ja', label: '日本語', lang: 'ja', dir: 'ltr' },
  { code: 'zh-Hans', path: 'zh-hans', label: '简体中文', lang: 'zh-CN', dir: 'ltr' },
  { code: 'ar', path: 'ar', label: 'العربية', lang: 'ar', dir: 'rtl' },
  { code: 'he', path: 'he', label: 'עברית', lang: 'he', dir: 'rtl' },
  { code: 'uk', path: 'uk', label: 'Українська', lang: 'uk', dir: 'ltr' },
  { code: 'bg', path: 'bg', label: 'Български', lang: 'bg', dir: 'ltr' },
  { code: 'pl', path: 'pl', label: 'Polski', lang: 'pl', dir: 'ltr' },
  { code: 'cs', path: 'cs', label: 'Čeština', lang: 'cs', dir: 'ltr' },
  { code: 'hr', path: 'hr', label: 'Hrvatski', lang: 'hr', dir: 'ltr' },
];

export const NON_DEFAULT_LOCALES = LOCALES.filter((locale) => locale.code !== DEFAULT_LOCALE);

export function localeInfo(code) {
  const locale = LOCALES.find((entry) => entry.code === code);
  if (!locale) throw new Error(`Unknown website locale: ${code}`);
  return locale;
}

export function localizePath(localeCode, path) {
  if (/^https?:\/\//.test(path) || path.startsWith('#')) return path;
  const locale = localeInfo(localeCode);
  const normalized = path.startsWith('/') ? path : `/${path}`;
  if (!locale.path) return normalized;
  return `/${locale.path}${normalized}`;
}
