import { DEFAULT_LOCALE, LOCALES, NON_DEFAULT_LOCALES, localeInfo, localizePath } from './locales.mjs';
import { sidebarTranslations } from './site-locales/sidebar.mjs';
import en from './site-locales/en.mjs';
import es from './site-locales/es.mjs';
import fr from './site-locales/fr.mjs';
import de from './site-locales/de.mjs';
import pt_BR from './site-locales/pt_br.mjs';
import ja from './site-locales/ja.mjs';
import ko from './site-locales/ko.mjs';
import zh_Hans from './site-locales/zh_hans.mjs';
import ar from './site-locales/ar.mjs';
import he from './site-locales/he.mjs';
import uk from './site-locales/uk.mjs';
import bg from './site-locales/bg.mjs';
import pl from './site-locales/pl.mjs';
import cs from './site-locales/cs.mjs';
import hr from './site-locales/hr.mjs';
import zh_Hant from './site-locales/zh_hant.mjs';
import it from './site-locales/it.mjs';
import tr from './site-locales/tr.mjs';
import vi from './site-locales/vi.mjs';
import nl from './site-locales/nl.mjs';
import hi from './site-locales/hi.mjs';
import bn from './site-locales/bn.mjs';
import ta from './site-locales/ta.mjs';
import te from './site-locales/te.mjs';
import mr from './site-locales/mr.mjs';
import ur from './site-locales/ur.mjs';
import gu from './site-locales/gu.mjs';
import kn from './site-locales/kn.mjs';
import ml from './site-locales/ml.mjs';
import pa from './site-locales/pa.mjs';
import th from './site-locales/th.mjs';

export { DEFAULT_LOCALE, LOCALES, NON_DEFAULT_LOCALES, localeInfo, localizePath, sidebarTranslations };

export const landing = {
  en: en,
  es: es,
  fr: fr,
  de: de,
  'pt-BR': pt_BR,
  ja: ja,
  ko: ko,
  'zh-Hans': zh_Hans,
  ar: ar,
  he: he,
  uk: uk,
  bg: bg,
  pl: pl,
  cs: cs,
  hr: hr,
  'zh-Hant': zh_Hant,
  it: it,
  tr: tr,
  vi: vi,
  nl: nl,
  hi: hi,
  bn: bn,
  ta: ta,
  te: te,
  mr: mr,
  ur: ur,
  gu: gu,
  kn: kn,
  ml: ml,
  pa: pa,
  th: th,
};

const fallback = landing.en;
for (const locale of LOCALES) {
  landing[locale.code] ??= fallback;
}
