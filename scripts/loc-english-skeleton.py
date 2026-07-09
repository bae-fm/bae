#!/usr/bin/env python3
"""Flag English-skeleton entries: an it/tr/vi/nl catalog value that is still
mostly the English source sentence, either untranslated or with a single
glossary noun swapped in (sometimes with an English suffix glued onto a
non-English stem, e.g. "Sincronizzazioneing", "Eşzamanlamaed", "Đồng bộed",
"Importerened").

Reads both xcstrings catalogs (strict — these gate CI) plus the Android
values-{it,tr,vi,nl}/strings.xml and Windows Strings/{it,tr,vi,nl}/Resources.resw
catalogs (report-only: the Android catalogs are audited by hand rather than
gated here, and the Windows resw catalogs carry pre-existing rot that hasn't
been swept, so gating on either would fail CI on defects this script cannot
yet fix).

Detectors:
  - glued morphology: an English suffix (ing/ed/s) glued onto a non-English
    stem — either (a) a word containing a non-ASCII letter immediately
    followed by the suffix, or (b) a known glossary-noun ending followed by
    the suffix. Locale-scoped: the ASCII-only variant of (a) false-positives
    outside it/tr/vi/nl (e.g. Czech "před").
  - English skeleton: 2 or more English function/content words from a
    per-locale safe list (loanwords and words that are also valid in the
    target language are excluded per locale).
  - token overlap >= 0.7: share of the en source's tokens (placeholders,
    format tokens, digits, and technical proper nouns stripped) that appear
    verbatim in the target value. Gates CI at this threshold. The 0.5-0.67
    band catches more real breakage but only at ~70% precision (loanword-
    heavy real translations live there), so it is report-only (--verbose).

Adjudicated-legitimate hits (loanwords, short strings that happen to overlap)
go in scripts/loc-skeleton-allowlist.txt as `locale\tkey` (or
`locale\tkey\tplural-form` for a plural variation), mirroring
loc-orphans-allowlist.txt's mechanics.

Also verifies, for every it/tr/vi/nl value: the placeholder multiset
(%@ / %lld / %n$… / {token}) matches its en source exactly. A mismatch is a
translation defect regardless of what the other detectors say, so it is not
allowlist-suppressible.

Gates CI: exits non-zero if any strict-detector or placeholder-multiset hit in
the two xcstrings catalogs is not allowlisted (placeholder mismatches are never
allowlist-suppressible).
"""
import json
import pathlib
import re
import sys
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
ALLOWLIST = ROOT / "scripts/loc-skeleton-allowlist.txt"

TARGET_LOCALES = ("it", "tr", "vi", "nl")

MAC_XCSTRINGS = "bae-macos/bae/bae/Localizable.xcstrings"
IOS_XCSTRINGS = "bae-ios/bae/bae/Localizable.xcstrings"
ANDROID_STRINGS = {
    loc: f"bae-android/app/src/main/res/values-{loc}/strings.xml" for loc in TARGET_LOCALES
}
WINDOWS_RESW = {
    loc: f"bae-windows/Strings/{loc}/Resources.resw" for loc in TARGET_LOCALES
}

# ── Placeholder / token stripping ───────────────────────────────────────────

PLACEHOLDER_RE = re.compile(r"%\d+\$[a-zA-Z@]|%lld|%@|%[sd]|\{[^}]+\}")
ICU_PLURAL_WORDS = {"plural", "one", "other", "few", "many", "zero", "#"}

TECHNICAL_PROPER_NOUNS = {
    "bae", "discogs", "musicbrainz", "oauth", "icloud", "itunes", "dropbox",
    "onedrive", "google", "drive", "mcp", "api", "id", "url", "s3", "finder",
    "mac", "macos", "ios", "android", "windows", "cloudkit", "json", "xml",
    "http", "https", "www", "com", "flac", "mp3", "m4a", "aac", "wav", "cue",
}

WORD_RE = re.compile(r"[A-Za-z][A-Za-z']*")


def strip_placeholders(s):
    return PLACEHOLDER_RE.sub(" ", s)


def placeholder_multiset(s):
    return sorted(PLACEHOLDER_RE.findall(s))


def tokenize(s):
    return [w.lower() for w in WORD_RE.findall(strip_placeholders(s))]


# ── Glued morphology ─────────────────────────────────────────────────────────

SUFFIX_RE = re.compile(r"(ing|ed|s)\b", re.UNICODE)
NON_ASCII_WORD_RE = re.compile(r"\b\w*[^\x00-\x7f]\w*(ing|ed|s)\b", re.UNICODE)
GLOSSARY_NOUN_ENDING_RE = re.compile(
    r"\b\w*(zione|zioni|atie|satie|eren|lama|leme|hóa|bộ|xuống|nhập)(ing|ed|s)\b",
    re.UNICODE | re.IGNORECASE,
)


def glued_morphology_hits(value):
    hits = set()
    for m in NON_ASCII_WORD_RE.finditer(value):
        hits.add(m.group(0))
    for m in GLOSSARY_NOUN_ENDING_RE.finditer(value):
        hits.add(m.group(0))
    return hits


# ── English skeleton (per-locale safe word lists) ──────────────────────────
# English function/content words common in this catalog's sentences, minus
# words that are also valid in the target language and minus loanwords the
# good entries already use (excluded so a real translation with a loanword
# doesn't flag). Each locale excludes its own overlaps.

_BASE_SAFE_WORDS = {
    "the", "this", "that", "will", "from", "with", "your", "you", "not",
    "and", "for", "again", "remove", "removes", "removed", "add", "added",
    "open", "close", "closed", "move", "moved", "new", "name", "named",
    "search", "release", "released", "track", "settings", "setting",
    "folder", "source", "provider", "next", "back", "restore", "restored",
    "failed", "couldn't", "cannot", "queue", "offline", "again", "now",
    "was", "were", "been", "being", "have", "has", "had", "does", "doesn't",
    "stop", "stopped", "start", "started", "keeps", "working", "session",
    "account", "device", "configuration", "confirm", "disconnect",
    "connected", "connect", "uploading", "downloading", "retrying",
    "pressing", "unlock", "locked", "via",
}

# Per-locale exclusions: real words in that language that collide with an
# English safe word, so they must not count as English-skeleton evidence.
_LOCALE_EXCLUDE = {
    "it": {"a", "in", "i", "e", "via", "or", "con"},
    "tr": {"a", "in", "e", "or", "and"},
    "vi": {"a", "in", "i", "e", "or", "con"},
    "nl": {"is", "was", "been", "of", "in", "a", "on", "met", "aan", "via", "account"},
}

# Loanwords the good entries already use — legitimate, never English-skeleton
# evidence.
_LOANWORDS = {
    "album", "albums", "file", "files", "cloud", "preset", "presets",
    "token", "download", "id", "backup", "app", "sync",
}

SAFE_WORDS = {}
for _loc in TARGET_LOCALES:
    SAFE_WORDS[_loc] = _BASE_SAFE_WORDS - _LOCALE_EXCLUDE[_loc] - _LOANWORDS


def english_skeleton_hits(value, locale):
    words = tokenize(value)
    safe = SAFE_WORDS[locale]
    return [w for w in words if w in safe]


# ── Token overlap ────────────────────────────────────────────────────────────


def token_overlap(en_value, target_value):
    en_tokens = [t for t in tokenize(en_value) if t not in TECHNICAL_PROPER_NOUNS
                 and not t.isdigit()]
    if not en_tokens:
        return None
    target_tokens = set(tokenize(target_value))
    shared = sum(1 for t in en_tokens if t in target_tokens)
    return shared / len(en_tokens)


# ── Allowlist ────────────────────────────────────────────────────────────────


def load_allowlist():
    if not ALLOWLIST.exists():
        return set()
    entries = set()
    for line in ALLOWLIST.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        locale, key = parts[0], parts[1]
        form = parts[2] if len(parts) > 2 else ""
        entries.add((locale, key, form))
    return entries


# ── xcstrings ─────────────────────────────────────────────────────────────────


def xcstrings_leaves(path):
    """Yield (key, plural_form_or_None, en_value, {locale: value}) for every
    stringUnit / plural-variation leaf in the catalog."""
    data = json.loads((ROOT / path).read_text(encoding="utf-8"))
    for key, entry in data["strings"].items():
        locs = entry.get("localizations", {})
        en = locs.get("en")
        if en is None:
            continue
        if "stringUnit" in en:
            en_value = en["stringUnit"]["value"]
            values = {}
            for loc in TARGET_LOCALES:
                lv = locs.get(loc)
                if lv and "stringUnit" in lv:
                    values[loc] = lv["stringUnit"]["value"]
            yield key, None, en_value, values
        elif "variations" in en:
            plural = en["variations"].get("plural", {})
            for form in plural:
                en_value = plural[form]["stringUnit"]["value"]
                values = {}
                for loc in TARGET_LOCALES:
                    lv = locs.get(loc)
                    if lv and "variations" in lv:
                        pf = lv["variations"].get("plural", {}).get(form)
                        if pf and "stringUnit" in pf:
                            values[loc] = pf["stringUnit"]["value"]
                yield key, form, en_value, values


# ── Android strings.xml ──────────────────────────────────────────────────────


def android_leaves(en_path, target_path, locale):
    if not (ROOT / en_path).exists() or not (ROOT / target_path).exists():
        return
    en_tree = ET.parse(ROOT / en_path)
    target_tree = ET.parse(ROOT / target_path)

    en_strings = {el.get("name"): "".join(el.itertext()) for el in en_tree.getroot().findall("string")}
    target_strings = {el.get("name"): "".join(el.itertext()) for el in target_tree.getroot().findall("string")}
    for name, en_value in en_strings.items():
        if name in target_strings:
            yield name, None, en_value, {locale: target_strings[name]}

    for en_plurals_el in en_tree.getroot().findall("plurals"):
        name = en_plurals_el.get("name")
        target_plurals_el = target_tree.getroot().find(f"./plurals[@name='{name}']")
        if target_plurals_el is None:
            continue
        target_items = {el.get("quantity"): "".join(el.itertext()) for el in target_plurals_el.findall("item")}
        for item in en_plurals_el.findall("item"):
            quantity = item.get("quantity")
            en_value = "".join(item.itertext())
            if quantity in target_items:
                yield name, quantity, en_value, {locale: target_items[quantity]}


# ── Windows .resw ────────────────────────────────────────────────────────────


def resw_leaves(en_path, target_path, locale):
    if not (ROOT / en_path).exists() or not (ROOT / target_path).exists():
        return
    ns = {}
    en_tree = ET.parse(ROOT / en_path)
    target_tree = ET.parse(ROOT / target_path)
    en_values = {}
    for data_el in en_tree.getroot().findall("data"):
        value_el = data_el.find("value")
        if value_el is not None and value_el.text is not None:
            en_values[data_el.get("name")] = value_el.text
    target_values = {}
    for data_el in target_tree.getroot().findall("data"):
        value_el = data_el.find("value")
        if value_el is not None and value_el.text is not None:
            target_values[data_el.get("name")] = value_el.text
    for name, en_value in en_values.items():
        if name in target_values:
            yield name, None, en_value, {locale: target_values[name]}


def strip_icu(s):
    # ICU plural syntax ("{count, plural, one {# item} other {# items}}") and
    # the `#` runtime substitution read as English-skeleton/overlap noise;
    # strip the keywords before scanning resw values (a measured
    # false-positive source in the Windows catalogs).
    words = WORD_RE.findall(s)
    for w in words:
        if w.lower() in ICU_PLURAL_WORDS:
            s = re.sub(rf"\b{re.escape(w)}\b", " ", s)
    return s.replace("#", " ")


# ── Scan orchestration ──────────────────────────────────────────────────────


def scan_leaves(leaves):
    """leaves: iterable of (key, form, en_value, {locale: value}).

    Returns (detector_hits, band_hits): detector_hits are glued-morphology /
    english-skeleton / token-overlap>=0.7 (the ~100%-precision detectors);
    band_hits are the 0.5-0.67 token-overlap band (~70% precision, report-only
    everywhere — real breakage lives there too, but so do enough legitimate
    loanword-heavy translations that it can't gate CI). Each is a list of
    dicts. Callers decide whether detector_hits gates (xcstrings) or only
    reports (Android/Windows), and whether to print band_hits at all (only
    under --verbose).
    """
    detector_hits = []
    band_hits = []
    for key, form, en_value, values in leaves:
        for locale, target_value in values.items():
            scan_value = strip_icu(target_value)
            reasons = []
            glued = glued_morphology_hits(scan_value)
            if glued:
                reasons.append(f"glued morphology: {sorted(glued)}")
            skeleton = english_skeleton_hits(scan_value, locale)
            if len(skeleton) >= 2:
                reasons.append(f"english skeleton words: {sorted(set(skeleton))}")
            overlap = token_overlap(en_value, scan_value)
            if overlap is not None and overlap >= 0.7:
                reasons.append(f"token overlap: {overlap:.2f}")

            hit = {
                "key": key,
                "form": form,
                "locale": locale,
                "en": en_value,
                "value": target_value,
                "reasons": reasons,
            }
            if reasons:
                detector_hits.append(hit)
            elif overlap is not None and 0.5 <= overlap < 0.7:
                band_hits.append({**hit, "reasons": [f"token overlap (report-only band): {overlap:.2f}"]})
    return detector_hits, band_hits


def placeholder_mismatches(leaves):
    mismatches = []
    for key, form, en_value, values in leaves:
        en_multiset = placeholder_multiset(en_value)
        for locale, target_value in values.items():
            target_multiset = placeholder_multiset(target_value)
            if target_multiset != en_multiset:
                mismatches.append({
                    "key": key, "form": form, "locale": locale,
                    "en": en_value, "value": target_value,
                    "en_placeholders": en_multiset, "value_placeholders": target_multiset,
                })
    return mismatches


def print_hits(label, hits, allowed):
    unallowed = [h for h in hits if (h["locale"], h["key"], h["form"] or "") not in allowed]
    print(f"=== {label}: {len(hits)} hit(s), {len(unallowed)} not allowlisted ===")
    for h in unallowed:
        loc_label = h["key"] if not h["form"] else f"{h['key']} [{h['form']}]"
        print(f"  [{h['locale']}] {loc_label!r}")
        print(f"      en:  {h['en']!r}")
        print(f"      val: {h['value']!r}")
        print(f"      reasons: {'; '.join(h['reasons'])}")
    return unallowed


def main():
    verbose = "--verbose" in sys.argv
    allowed = load_allowlist()

    total_gating_failures = 0

    # ── Strict: the two xcstrings catalogs ──────────────────────────────────
    for label, path in (("macOS xcstrings", MAC_XCSTRINGS), ("iOS xcstrings", IOS_XCSTRINGS)):
        leaves = list(xcstrings_leaves(path))
        detector_hits, band_hits = scan_leaves(leaves)
        unallowed = print_hits(f"{label} (strict)", detector_hits, allowed)
        total_gating_failures += len(unallowed)

        mismatches = placeholder_mismatches(leaves)
        if mismatches:
            print(f"=== {label}: {len(mismatches)} placeholder-multiset mismatch(es) ===")
            for m in mismatches:
                loc_label = m["key"] if not m["form"] else f"{m['key']} [{m['form']}]"
                print(f"  [{m['locale']}] {loc_label!r}")
                print(f"      en:  {m['en']!r} -> {m['en_placeholders']}")
                print(f"      val: {m['value']!r} -> {m['value_placeholders']}")
            total_gating_failures += len(mismatches)

        if verbose:
            print_hits(f"{label} (report-only band)", band_hits, allowed)

    # ── Report-only: Android (strict coverage lands in its own sweep PR) ────
    android_leaves_all = []
    for loc, target_path in ANDROID_STRINGS.items():
        en_path = "bae-android/app/src/main/res/values/strings.xml"
        android_leaves_all.extend(android_leaves(en_path, target_path, loc))
    if android_leaves_all:
        detector_hits, band_hits = scan_leaves(android_leaves_all)
        print_hits("Android strings.xml (report-only)", detector_hits, allowed)
        if verbose:
            print_hits("Android strings.xml (report-only band)", band_hits, allowed)

    # ── Report-only: Windows resw (rot confirmed, sweep not yet scheduled) ──
    windows_leaves_all = []
    for loc, target_path in WINDOWS_RESW.items():
        en_path = "bae-windows/Strings/en-US/Resources.resw"
        windows_leaves_all.extend(resw_leaves(en_path, target_path, loc))
    if windows_leaves_all:
        detector_hits, band_hits = scan_leaves(windows_leaves_all)
        print_hits("Windows resw (report-only)", detector_hits, allowed)
        if verbose:
            print_hits("Windows resw (report-only band)", band_hits, allowed)

    print(f"\nTOTAL gating failures: {total_gating_failures} (allowlist: {len(allowed)})")
    if total_gating_failures:
        print(
            "\nEnglish-skeleton or placeholder-mismatch entries found in a gated "
            "catalog — translate them, or if a hit is a legitimate loanword/"
            f"short-string false positive, add it to {ALLOWLIST.name}.",
            file=sys.stderr,
        )
    return 1 if total_gating_failures else 0


if __name__ == "__main__":
    sys.exit(main())
