export const DEFAULT_LOCALE = 'en';

export const LOCALES = [
  { code: 'en', path: '', label: 'English', lang: 'en', dir: 'ltr' },
  { code: 'es', path: 'es', label: 'Español', lang: 'es', dir: 'ltr' },
  { code: 'fr', path: 'fr', label: 'Français', lang: 'fr', dir: 'ltr' },
  { code: 'de', path: 'de', label: 'Deutsch', lang: 'de', dir: 'ltr' },
  { code: 'pt-BR', path: 'pt-br', label: 'Português do Brasil', lang: 'pt-BR', dir: 'ltr' },
  { code: 'ja', path: 'ja', label: '日本語', lang: 'ja', dir: 'ltr' },
  { code: 'ko', path: 'ko', label: '한국어', lang: 'ko', dir: 'ltr' },
  { code: 'zh-Hans', path: 'zh-hans', label: '简体中文', lang: 'zh-CN', dir: 'ltr' },
  { code: 'zh-Hant', path: 'zh-hant', label: '繁體中文', lang: 'zh-Hant', dir: 'ltr' },
  { code: 'ar', path: 'ar', label: 'العربية', lang: 'ar', dir: 'rtl' },
  { code: 'he', path: 'he', label: 'עברית', lang: 'he', dir: 'rtl' },
  { code: 'uk', path: 'uk', label: 'Українська', lang: 'uk', dir: 'ltr' },
  { code: 'bg', path: 'bg', label: 'Български', lang: 'bg', dir: 'ltr' },
  { code: 'pl', path: 'pl', label: 'Polski', lang: 'pl', dir: 'ltr' },
  { code: 'cs', path: 'cs', label: 'Čeština', lang: 'cs', dir: 'ltr' },
  { code: 'hr', path: 'hr', label: 'Hrvatski', lang: 'hr', dir: 'ltr' },
  { code: 'it', path: 'it', label: 'Italiano', lang: 'it', dir: 'ltr' },
  { code: 'tr', path: 'tr', label: 'Türkçe', lang: 'tr', dir: 'ltr' },
  { code: 'vi', path: 'vi', label: 'Tiếng Việt', lang: 'vi', dir: 'ltr' },
  { code: 'nl', path: 'nl', label: 'Nederlands', lang: 'nl', dir: 'ltr' },
  { code: 'hi', path: 'hi', label: 'हिन्दी', lang: 'hi', dir: 'ltr' },
  { code: 'bn', path: 'bn', label: 'বাংলা', lang: 'bn', dir: 'ltr' },
  { code: 'ta', path: 'ta', label: 'தமிழ்', lang: 'ta', dir: 'ltr' },
  { code: 'te', path: 'te', label: 'తెలుగు', lang: 'te', dir: 'ltr' },
  { code: 'mr', path: 'mr', label: 'मराठी', lang: 'mr', dir: 'ltr' },
  { code: 'ur', path: 'ur', label: 'اردو', lang: 'ur', dir: 'rtl' },
  { code: 'gu', path: 'gu', label: 'ગુજરાતી', lang: 'gu', dir: 'ltr' },
  { code: 'kn', path: 'kn', label: 'ಕನ್ನಡ', lang: 'kn', dir: 'ltr' },
  { code: 'ml', path: 'ml', label: 'മലയാളം', lang: 'ml', dir: 'ltr' },
  { code: 'pa', path: 'pa', label: 'ਪੰਜਾਬੀ', lang: 'pa', dir: 'ltr' },
  { code: 'th', path: 'th', label: 'ไทย', lang: 'th', dir: 'ltr' },
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
