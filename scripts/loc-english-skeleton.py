#!/usr/bin/env python3
"""Flag English-skeleton entries: a catalog value that is still mostly the
English source sentence, either untranslated or with a single glossary noun
swapped in (sometimes with an English suffix glued onto a non-English stem,
e.g. "Sincronizzazioneing", "Eşzamanlamaed", "Importerened").

Covers every locale bae ships — the set is bae-loc's TARGET_LOCALES, so this script keeps no list of its own. Reads the
macOS xcstrings catalog and the Android values-<locale>/strings.xml catalogs;
both gate CI. A locale with no chrome file of a given kind simply has nothing
to scan there.

Detectors:
  - glued morphology: an English suffix (ing/ed/s) welded onto a target-
    language stem. Whether that is even distinguishable from native
    inflection is a fact about the language — French "métadonnées" and Czech
    "před" are ordinary words — so the detector runs only for locales in
    GLUED_MORPHOLOGY_LOCALES, which carry a table of their own noun endings.
  - English skeleton: 2 or more English function/content words from a
    per-locale safe list (loanwords and words that are also valid in the
    target language are excluded per locale). Only ASCII tokens count; a
    target-language word is never English evidence.
  - token overlap >= 0.7: share of the en source's tokens (placeholders,
    format tokens, digits, and technical proper nouns stripped) that appear
    verbatim in the target value. Needs at least two such tokens — a one-word
    source that survives translation says nothing. Gates CI at this
    threshold. The 0.5-0.67 band catches more real breakage but only at ~70%
    precision (loanword-heavy real translations live there), so it is
    report-only (--verbose).

Adjudicated-legitimate hits (loanwords, short strings that happen to overlap)
go in scripts/loc-skeleton-allowlist.txt as `locale\tkey` (or
`locale\tkey\tplural-form` for a plural variation), mirroring
loc-orphans-allowlist.txt's mechanics.

Also verifies, for every value: the placeholder multiset
(%@ / %lld / %n$… / {token}) matches its en source exactly (a zero, one or
two plural branch may spell its count out), and it carries no ICU `#` count
placeholder, which neither catalog format substitutes. A mismatch is a
translation defect regardless of what the other detectors say, so it is not
allowlist-suppressible.

Also refuses an ellipsis written as three full stops ("..." for "…",
spec/words.md "Punctuation") in any key, source or translation of either
catalog; that is never allowlist-suppressible either.

Gates CI: exits non-zero if any strict-detector or placeholder-multiset hit in
the macOS xcstrings catalog or the Android catalogs is not allowlisted
(placeholder mismatches are never allowlist-suppressible).
"""
import json
import pathlib
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
ALLOWLIST = ROOT / "scripts/loc-skeleton-allowlist.txt"


def shipping_locales():
    """The locale set bae ships, read from the TARGET_LOCALES literal in
    bae-loc's lib.rs. Reading the source keeps this check from compiling
    bae-loc (`loc-gen locales` prints the same list, but running it builds
    the crate and its dependencies, which a hook run pays in full whenever
    that build is cold)."""
    source = (ROOT / "bae-loc/src/lib.rs").read_text(encoding="utf-8")
    block = re.search(r"pub const TARGET_LOCALES: &\[&str\] = &\[(.*?)\];", source, re.DOTALL)
    if not block:
        sys.exit("bae-loc/src/lib.rs: no TARGET_LOCALES literal found")
    locales = tuple(re.findall(r'"([^"]+)"', block.group(1)))
    if not locales:
        sys.exit("bae-loc/src/lib.rs: TARGET_LOCALES is empty")
    return locales


def android_values_dir(locale):
    """The Android resource qualifier carrying `locale`, mirroring bae-loc's
    emitter: a bare language is `values-<lang>`, anything with a region or
    script subtag needs the BCP-47 `b+` form."""
    if "-" in locale:
        lang, rest = locale.split("-", 1)
        return f"values-b+{lang}+{rest}"
    return f"values-{locale}"


TARGET_LOCALES = shipping_locales()

MAC_XCSTRINGS = "bae-macos/bae/bae/Localizable.xcstrings"
ANDROID_EN_STRINGS = "bae-android/app/src/main/res/values/strings.xml"
ANDROID_STRINGS = {
    loc: f"bae-android/app/src/main/res/{android_values_dir(loc)}/strings.xml"
    for loc in TARGET_LOCALES
}

# ── Placeholder / token stripping ───────────────────────────────────────────

# `%1$lld` (or `%1$llu`) is one token, not `%1$l` plus the letters "ld": the specifier is
# matched whole so a positional integer argument reads as an integer.
PLACEHOLDER_RE = re.compile(r"%\d+\$(?:ll[du]|[a-zA-Z@])|%ll[du]|%@|%[sd]|\{[^}]+\}")

# A URL is the same in every language, and its path segments are English words
# nobody translated ("…/settings/developers"). Strip it before tokenizing, or a
# correct translation that keeps the link reads as English skeleton.
URL_RE = re.compile(r"\bhttps?://\S+", re.IGNORECASE)

TECHNICAL_PROPER_NOUNS = {
    "bae", "discogs", "musicbrainz", "oauth", "icloud", "itunes", "dropbox",
    "onedrive", "mcp", "api", "id", "url", "s3", "finder",
    "mac", "macos", "ios", "android", "windows", "cloudkit", "json", "xml",
    "http", "https", "www", "com", "flac", "mp3", "m4a", "aac", "wav", "cue",
    # Formats, units, and protocol names: identical in every locale by nature,
    # so their survival into a translation is not evidence of anything.
    "alac", "aiff", "ogg", "opus", "dsd", "dsf", "pcm", "mqa", "ape",
    "bit", "bits", "khz", "hz", "kbps", "kb", "mb", "gb", "tb", "ms",
    "cd", "dvd", "sacd", "usb", "ip", "lan", "wifi", "dlna", "upnp",
    "airplay", "chromecast", "sonos", "subsonic", "isrc", "upc", "ean",
    "ocr", "toc",
}

# Unicode-aware: a target-language word is one token, not an ASCII fragment
# plus a stray letter. Splitting "Hasło" into "Has" + "o" invents an English
# "has" that was never there, and pollutes the overlap token set besides.
WORD_RE = re.compile(r"[^\W\d_]+", re.UNICODE)


def strip_placeholders(s):
    return PLACEHOLDER_RE.sub(" ", s)


def placeholder_multiset(s):
    return sorted(PLACEHOLDER_RE.findall(s))


_INT_SPECIFIERS = ("lld", "llu", "d")


def positional_signature(s):
    """The arguments a value substitutes, keyed by position rather than by
    order of appearance.

    Word order is the whole point of translating, so a locale that puts the
    second argument first writes `%2$@ … %1$@` where English wrote `%@ … %@`.
    Both substitute the same two arguments; comparing raw token order would
    call the correct translation a defect. Apple and Android both number bare
    specifiers by appearance, so this assigns those positions the same way and
    compares what is left: {position or name: specifier}."""
    signature = {}
    implicit = 0
    for token in PLACEHOLDER_RE.findall(s):
        if token.startswith("{"):
            signature[token] = "brace"
            continue
        if "$" in token:
            index, specifier = token[1:].split("$", 1)
            signature[int(index)] = specifier
            continue
        implicit += 1
        signature[implicit] = token[1:]
    return signature


# The plural categories that can each stand for a single number, whose branch
# may spell the count out instead of printing it. "few", "many" and "other"
# always cover several numbers, so their branches must print the count.
_SINGLE_NUMBER_FORMS = ("zero", "one", "two")


def placeholders_match(en_value, target_value, plural_form):
    """Same arguments, whatever order the target puts them in.

    `plural_form` names the branch of a plural (an .xcstrings plural
    variation, an Android `<plurals>` item) the value is, or is None. In a
    zero, one or two branch the count itself may be spelled out — Arabic's
    "one" branch reads "ملف واحد", not "1 ملف" — so that branch may drop the
    integer argument. Every other argument must be present; dropping one
    means the value can't say what it promises."""
    en_signature = positional_signature(en_value)
    target_signature = positional_signature(target_value)
    if en_signature == target_signature:
        return True
    if plural_form not in _SINGLE_NUMBER_FORMS:
        return False
    dropped = {k: v for k, v in en_signature.items() if k not in target_signature}
    return (
        len(dropped) == 1
        and next(iter(dropped.values())) in _INT_SPECIFIERS
        and all(target_signature[k] == en_signature[k] for k in target_signature)
    )


def icu_count_placeholder(en_value, target_value):
    """A `#` the English source doesn't have is ICU MessageFormat's count
    placeholder. Neither String Catalogs nor Android resources substitute it,
    so the person reads a literal "#"."""
    return "#" in target_value and "#" not in en_value


def tokenize(s):
    return [w.lower() for w in WORD_RE.findall(URL_RE.sub(" ", strip_placeholders(s)))]


# ── Glued morphology ─────────────────────────────────────────────────────────

# An English suffix on a word only reads as glued-on where it is not also that
# language's own inflection: -s ends ordinary French, Spanish, Portuguese, and
# German plurals, and -ed ends Czech "před". So this detector is scoped to the
# locales whose morphology it was written against, each of which contributes
# the noun endings a mangled word gets welded onto. A locale joins the table
# when a glued form is actually found in it; the others are covered by the
# placeholder and token-overlap checks.
GLOSSARY_NOUN_ENDINGS = {
    "it": ("zione", "zioni"),
    "nl": ("atie", "satie", "eren"),
    "tr": ("lama", "leme"),
}
GLUED_MORPHOLOGY_LOCALES = tuple(GLOSSARY_NOUN_ENDINGS)

# The curly apostrophe is non-ASCII but not a foreign letter ("Regio’s").
NON_ASCII_WORD_RE = re.compile(r"\b\w*[^\x00-\x7f\u2019]\w*(ing|ed|s)\b", re.UNICODE)
GLOSSARY_NOUN_ENDING_RE = re.compile(
    r"\b\w*("
    + "|".join(sorted({e for es in GLOSSARY_NOUN_ENDINGS.values() for e in es}))
    + r")(ing|ed|s)\b",
    re.UNICODE | re.IGNORECASE,
)


def glued_morphology_hits(value, locale):
    if locale not in GLUED_MORPHOLOGY_LOCALES:
        return set()
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
# English safe word, so they must not count as English-skeleton evidence. A
# locale earns an entry when one of its correct translations trips the
# detector; an absent entry means none has.
_LOCALE_EXCLUDE = {
    "it": {"a", "in", "i", "e", "via", "or", "con"},
    "tr": {"a", "in", "e", "or", "and"},
    "nl": {"is", "was", "been", "of", "in", "a", "on", "met", "aan", "via", "account"},
    "da": {"for", "session", "start"},
    "nb": {"for", "start"},
}

# Loanwords the good entries already use — legitimate, never English-skeleton
# evidence.
_LOANWORDS = {
    "album", "albums", "file", "files", "cloud", "preset", "presets",
    "token", "download", "id", "backup", "app", "sync",
}

SAFE_WORDS = {
    loc: _BASE_SAFE_WORDS - _LOCALE_EXCLUDE.get(loc, set()) - _LOANWORDS
    for loc in TARGET_LOCALES
}


def english_skeleton_hits(value, locale):
    safe = SAFE_WORDS[locale]
    return [w for w in tokenize(value) if w.isascii() and w in safe]


# ── Token overlap ────────────────────────────────────────────────────────────


# A source of one content word carries no signal: "Import" surviving as
# "Import" is a loanword, not an untranslated sentence, and every such string
# would otherwise score 1.00.
MIN_OVERLAP_TOKENS = 2


def token_overlap(en_value, target_value):
    en_tokens = [t for t in tokenize(en_value)
                 if t.isascii() and t not in TECHNICAL_PROPER_NOUNS
                 and not t.isdigit()]
    if len(en_tokens) < MIN_OVERLAP_TOKENS:
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
    stringUnit / plural-variation leaf in the catalog.

    The key is the English source when the catalog keeps no en slot. A locale
    may vary by plural where English doesn't ("%lld selected" is one English
    form, several Polish ones); each of its plural leaves is held against the
    one English value."""
    data = json.loads((ROOT / path).read_text(encoding="utf-8"))
    for key, entry in data["strings"].items():
        locs = entry.get("localizations", {})
        en = locs.get("en", {"stringUnit": {"value": key}})
        if "stringUnit" in en:
            en_value = en["stringUnit"]["value"]
            values = {}
            for loc in TARGET_LOCALES:
                lv = locs.get(loc)
                if lv and "stringUnit" in lv:
                    values[loc] = lv["stringUnit"]["value"]
                elif lv and "variations" in lv:
                    for form, pf in lv["variations"].get("plural", {}).items():
                        yield key, form, en_value, {loc: pf["stringUnit"]["value"]}
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


# ── Changed entries (hook runs) ─────────────────────────────────────────────


def head_text(path):
    """The file's text at HEAD, or None when HEAD has no such file."""
    out = subprocess.run(
        ["git", "show", f"HEAD:{path}"], cwd=ROOT, capture_output=True, text=True
    )
    return out.stdout if out.returncode == 0 else None


def xcstrings_entries(text):
    return json.loads(text)["strings"] if text else {}


def android_entries(text):
    """{name: value(s)} for the strings and plurals of a strings.xml text."""
    if not text:
        return {}
    root = ET.fromstring(text)
    entries = {el.get("name"): "".join(el.itertext()) for el in root.findall("string")}
    for el in root.findall("plurals"):
        entries[el.get("name")] = tuple(
            (item.get("quantity"), "".join(item.itertext())) for item in el.findall("item")
        )
    return entries


def changed_keys(paths):
    """The keys whose entries in the given catalog files differ from HEAD's.
    A file not among the catalogs contributes nothing."""
    keys = set()
    for path in paths:
        rel = str(pathlib.Path(path).resolve().relative_to(ROOT))
        if rel == MAC_XCSTRINGS:
            read = xcstrings_entries
        elif rel in ANDROID_STRINGS.values() or rel == ANDROID_EN_STRINGS:
            read = android_entries
        else:
            continue
        now_file = ROOT / rel
        now = read(now_file.read_text(encoding="utf-8") if now_file.exists() else None)
        before = read(head_text(rel))
        keys |= {k for k in now.keys() | before.keys() if now.get(k) != before.get(k)}
    return keys


# ── Scan orchestration ──────────────────────────────────────────────────────


def scan_leaves(leaves):
    """leaves: iterable of (key, form, en_value, {locale: value}).

    Returns (detector_hits, band_hits): detector_hits are glued-morphology /
    english-skeleton / token-overlap>=0.7 (the ~100%-precision detectors);
    band_hits are the 0.5-0.67 token-overlap band (~70% precision, report-only
    everywhere — real breakage lives there too, but so do enough legitimate
    loanword-heavy translations that it can't gate CI). Each is a list of
    dicts. Callers decide whether detector_hits gates (xcstrings) or only
    reports (Android), and whether to print band_hits at all (only
    under --verbose).
    """
    detector_hits = []
    band_hits = []
    for key, form, en_value, values in leaves:
        for locale, target_value in values.items():
            reasons = []
            glued = glued_morphology_hits(target_value, locale)
            if glued:
                reasons.append(f"glued morphology: {sorted(glued)}")
            skeleton = english_skeleton_hits(target_value, locale)
            if len(skeleton) >= 2:
                reasons.append(f"english skeleton words: {sorted(set(skeleton))}")
            overlap = token_overlap(en_value, target_value)
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
            if not placeholders_match(
                en_value, target_value, form
            ) or icu_count_placeholder(en_value, target_value):
                mismatches.append({
                    "key": key, "form": form, "locale": locale,
                    "en": en_value, "value": target_value,
                    "en_placeholders": en_multiset, "value_placeholders": target_multiset,
                })
    return mismatches


# An English sentence is never broken with a dash: a pause is a comma, a new
# thought a new sentence (projects/bae-rules/no-dash-as-sentence-punctuation).
# Only the source is held to it; Ukrainian, for one, writes its copula as a dash.
SENTENCE_DASH_RE = re.compile(r"—| – ")


def dashed_sources(leaves):
    """The English source values that use a dash as sentence punctuation."""
    return sorted({
        (key, en_value) for key, _form, en_value, _values in leaves
        if SENTENCE_DASH_RE.search(en_value)
    })


# An ellipsis is one character, "…", in the source and every translation
# (spec/words.md "Punctuation"); three full stops never stand for it.
THREE_DOTS = "..."


def three_dot_values(leaves):
    """Every (locale, key, value) that writes an ellipsis as three full stops,
    the key itself included: a String Catalog's key is its English source."""
    found = set()
    for key, _form, en_value, values in leaves:
        if THREE_DOTS in key or THREE_DOTS in en_value:
            found.add(("en", key, en_value if THREE_DOTS in en_value else key))
        for locale, value in values.items():
            if THREE_DOTS in value:
                found.add((locale, key, value))
    return sorted(found)


def print_three_dots(label, found):
    if found:
        print(f'=== {label}: {len(found)} value(s) writing an ellipsis as "..." ===')
        for locale, key, value in found:
            print(f"  [{locale}] {key!r}: {value!r}")
    return len(found)


# An apostrophe is the curly "’" in the source and every translation
# (spec/words.md "Punctuation"); the straight "'" never stands for it.
STRAIGHT_APOSTROPHE = "'"


def straight_apostrophe_values(leaves):
    """Every (locale, key, value) that writes an apostrophe as "'", the key
    itself included: a String Catalog's key is its English source."""
    found = set()
    for key, _form, en_value, values in leaves:
        if STRAIGHT_APOSTROPHE in key or STRAIGHT_APOSTROPHE in en_value:
            found.add(("en", key, en_value if STRAIGHT_APOSTROPHE in en_value else key))
        for locale, value in values.items():
            if STRAIGHT_APOSTROPHE in value:
                found.add((locale, key, value))
    return sorted(found)


def print_straight_apostrophes(label, found):
    if found:
        print(f"=== {label}: {len(found)} value(s) writing an apostrophe as \"'\" ===")
        for locale, key, value in found:
            print(f"  [{locale}] {key!r}: {value!r}")
    return len(found)


def print_dashed(label, dashed):
    if dashed:
        print(f"=== {label}: {len(dashed)} English source(s) with a sentence dash ===")
        for key, en_value in dashed:
            print(f"  {key!r}: {en_value!r}")
    return len(dashed)


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
    """With catalog paths as arguments (the hook's staged files), only the
    entries that differ from HEAD's are scanned; with none, every entry."""
    verbose = "--verbose" in sys.argv
    paths = [a for a in sys.argv[1:] if not a.startswith("--")]
    only = changed_keys(paths) if paths else None

    def narrowed(leaves):
        leaves = list(leaves)
        return leaves if only is None else [l for l in leaves if l[0] in only]

    allowed = load_allowlist()

    total_gating_failures = 0

    # ── Strict: the macOS xcstrings catalog and the Android catalog ─────────
    for label, path in (("macOS xcstrings", MAC_XCSTRINGS),):
        leaves = narrowed(xcstrings_leaves(path))
        detector_hits, band_hits = scan_leaves(leaves)
        unallowed = print_hits(f"{label} (strict)", detector_hits, allowed)
        total_gating_failures += len(unallowed)

        total_gating_failures += print_dashed(label, dashed_sources(leaves))
        total_gating_failures += print_three_dots(label, three_dot_values(leaves))
        total_gating_failures += print_straight_apostrophes(
            label, straight_apostrophe_values(leaves)
        )

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

    android_leaves_all = []
    for loc, target_path in ANDROID_STRINGS.items():
        en_path = ANDROID_EN_STRINGS
        android_leaves_all.extend(narrowed(android_leaves(en_path, target_path, loc)))
    if android_leaves_all:
        detector_hits, band_hits = scan_leaves(android_leaves_all)
        unallowed = print_hits("Android strings.xml (strict)", detector_hits, allowed)
        total_gating_failures += len(unallowed)

        total_gating_failures += print_dashed(
            "Android strings.xml", dashed_sources(android_leaves_all)
        )
        total_gating_failures += print_three_dots(
            "Android strings.xml", three_dot_values(android_leaves_all)
        )
        total_gating_failures += print_straight_apostrophes(
            "Android strings.xml", straight_apostrophe_values(android_leaves_all)
        )

        mismatches = placeholder_mismatches(android_leaves_all)
        if mismatches:
            print(f"=== Android strings.xml: {len(mismatches)} placeholder-multiset mismatch(es) ===")
            for m in mismatches:
                loc_label = m["key"] if not m["form"] else f"{m['key']} [{m['form']}]"
                print(f"  [{m['locale']}] {loc_label!r}")
                print(f"      en:  {m['en']!r} -> {m['en_placeholders']}")
                print(f"      val: {m['value']!r} -> {m['value_placeholders']}")
            total_gating_failures += len(mismatches)

        if verbose:
            print_hits("Android strings.xml (report-only band)", band_hits, allowed)

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
