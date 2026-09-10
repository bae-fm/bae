#!/usr/bin/env python3
"""Flag English-skeleton entries: a catalog value that is still mostly the
English source sentence, either untranslated or with a single glossary noun
swapped in (sometimes with an English suffix glued onto a non-English stem,
e.g. "Sincronizzazioneing", "Eşzamanlamaed", "Importerened").

Covers every locale bae ships — the set comes from `loc-gen locales`, i.e.
bae-loc's TARGET_LOCALES, so this script keeps no list of its own. Reads both
xcstrings catalogs, the Android values-<locale>/strings.xml catalogs, and the
Avalonia app's Strings/Resources.<locale>.resx catalogs; all four gate CI. A
locale with no chrome file of a given kind simply has nothing to scan there.

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
(%@ / %lld / %n$… / {token}) matches its en source exactly. A mismatch is a
translation defect regardless of what the other detectors say, so it is not
allowlist-suppressible.

Gates CI: exits non-zero if any strict-detector or placeholder-multiset hit in
the two xcstrings catalogs, the Android catalogs, or the ResX catalogs is not
allowlisted (placeholder mismatches are never allowlist-suppressible).
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
    """The locale set bae ships, straight from bae-loc's TARGET_LOCALES."""
    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "bae-loc", "--bin", "loc-gen", "--", "locales"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        sys.exit(f"`loc-gen locales` failed:\n{out.stderr}")
    locales = tuple(line.strip() for line in out.stdout.splitlines() if line.strip())
    if not locales:
        sys.exit("`loc-gen locales` printed nothing")
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
IOS_XCSTRINGS = "bae-ios/bae/bae/Localizable.xcstrings"
ANDROID_STRINGS = {
    loc: f"bae-android/app/src/main/res/{android_values_dir(loc)}/strings.xml"
    for loc in TARGET_LOCALES
}
RESX_CHROME = {
    loc: f"bae-avalonia/Strings/Resources.{loc}.resx" for loc in TARGET_LOCALES
}
RESX_CHROME_EN = "bae-avalonia/Strings/Resources.resx"

# ── Placeholder / token stripping ───────────────────────────────────────────

# `%1$lld` is one token, not `%1$l` plus the letters "ld": the specifier is
# matched whole so a positional integer argument reads as an integer.
PLACEHOLDER_RE = re.compile(r"%\d+\$(?:lld|[a-zA-Z@])|%lld|%@|%[sd]|\{[^}]+\}")

# A URL is the same in every language, and its path segments are English words
# nobody translated ("…/settings/developers"). Strip it before tokenizing, or a
# correct translation that keeps the link reads as English skeleton.
URL_RE = re.compile(r"\bhttps?://\S+", re.IGNORECASE)
ICU_PLURAL_WORDS = {"plural", "one", "other", "few", "many", "zero", "#"}

TECHNICAL_PROPER_NOUNS = {
    "bae", "discogs", "musicbrainz", "oauth", "icloud", "itunes", "dropbox",
    "onedrive", "google", "drive", "mcp", "api", "id", "url", "s3", "finder",
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


_INT_SPECIFIERS = ("lld", "d")


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


def placeholders_match(en_value, target_value, plural_leaf):
    """Same arguments, whatever order the target puts them in.

    `plural_leaf` is a single branch of a plural (an .xcstrings plural
    variation, an Android `<plurals>` item). There the count itself may be
    spelled out — Arabic's "one" branch reads "ملف واحد", not "1 ملف" — so a
    branch may drop the integer argument. Every other argument must be
    present; dropping one means the value can't say what it promises."""
    en_signature = positional_signature(en_value)
    target_signature = positional_signature(target_value)
    if en_signature == target_signature:
        return True
    if not plural_leaf:
        return False
    dropped = {k: v for k, v in en_signature.items() if k not in target_signature}
    return (
        len(dropped) == 1
        and next(iter(dropped.values())) in _INT_SPECIFIERS
        and all(target_signature[k] == en_signature[k] for k in target_signature)
    )


# The ResX catalogs embed an entire ICU plural expression as one string value
# (`{count, plural, one {# item} other {# items}}`); xcstrings and Android
# instead split each plural form into its own leaf before it ever reaches
# placeholder_multiset, so their values never contain a nested `{...}`. The
# flat PLACEHOLDER_RE above can't parse that nesting — it matches from the
# outer `{` to the first `}` it finds, which lands inside the first branch
# and turns that branch's translated words into a bogus "placeholder" token,
# so two branches with different words (e.g. "trovato" vs "trovati") read as
# a placeholder mismatch even though every real placeholder matches. The
# extractor below walks the string with balanced-brace matching, descends
# into a `{ARG, plural, ...}` construct's branches, and collects the actual
# placeholder tokens (`%...`, `{name}`, `#`) wherever they occur, including
# nested inside a branch; the plural argument name and branch keywords
# (one/other/...) are ICU control syntax, not placeholders.
_RESW_SIMPLE_FMT_RE = re.compile(r"%\d+\$(?:lld|[a-zA-Z@])|%lld|%@|%[sd]")
_RESW_PLURAL_HEADER_RE = re.compile(r"\s*\w+\s*,\s*plural\s*,\s*")


def _find_balanced_close(s, open_idx):
    depth = 0
    for i in range(open_idx, len(s)):
        if s[i] == "{":
            depth += 1
        elif s[i] == "}":
            depth -= 1
            if depth == 0:
                return i
    raise ValueError(f"unbalanced braces in {s!r}")


def _extract_mf1_placeholders(s):
    tokens = []
    i, n = 0, len(s)
    while i < n:
        c = s[i]
        if c == "#":
            tokens.append("#")
            i += 1
            continue
        m = _RESW_SIMPLE_FMT_RE.match(s, i)
        if m:
            tokens.append(m.group(0))
            i = m.end()
            continue
        if c == "{":
            close = _find_balanced_close(s, i)
            inner = s[i + 1:close]
            pm = _RESW_PLURAL_HEADER_RE.match(inner)
            if pm:
                tokens.extend(_extract_mf1_plural_branches(inner[pm.end():]))
            else:
                tokens.append("{" + inner + "}")
            i = close + 1
            continue
        i += 1
    return tokens


def _parse_mf1_plural_branches(s):
    """s is the branch-list portion of a plural construct, after the
    argument name and "plural," keyword: a sequence of `label {branch}`
    pairs (one/other/few/many/zero/=N). Returns {label: sorted placeholder
    tokens found in that branch's own text}."""
    branches = {}
    i, n = 0, len(s)
    while i < n:
        while i < n and s[i].isspace():
            i += 1
        if i >= n:
            break
        j = i
        while j < n and s[j] not in "{ \t\n":
            j += 1
        label = s[i:j]
        k = j
        while k < n and s[k].isspace():
            k += 1
        if k < n and s[k] == "{":
            close = _find_balanced_close(s, k)
            branches[label] = sorted(_extract_mf1_placeholders(s[k + 1:close]))
            i = close + 1
        else:
            i = j + 1 if j > i else i + 1
    return branches


def _extract_mf1_plural_branches(s):
    tokens = []
    for branch_tokens in _parse_mf1_plural_branches(s).values():
        tokens.extend(branch_tokens)
    return tokens


def _parse_top_level_plural(value):
    """If value is, in its entirety, a single `{ARG, plural, label {branch}
    ...}` construct (every plural value in this catalog is — the whole
    resource value, no surrounding text), return {label: sorted placeholder
    tokens in that branch}. Otherwise return None so the caller falls back
    to flat placeholder-multiset comparison."""
    s = value.strip()
    if not s.startswith("{"):
        return None
    close = _find_balanced_close(s, 0)
    if close != len(s) - 1:
        return None
    inner = s[1:close]
    pm = _RESW_PLURAL_HEADER_RE.match(inner)
    if not pm:
        return None
    return _parse_mf1_plural_branches(inner[pm.end():])


def mf1_placeholder_multiset(s):
    return sorted(_extract_mf1_placeholders(s))


def mf1_placeholders_match(en_value, target_value, plural_leaf=False):
    """Placeholder equality for an MF1 value, aware of two ways a correct
    translation legitimately differs from its English source inside a plural.

    A locale's CLDR categories are its own: Arabic writes zero/two/few/many
    where English has only one/other, and Japanese has only other. So a target
    branch is held against the English branch of the same label, or against
    English's `other` for a category English doesn't have.

    Within a branch, `#` — the count itself — may be spelled out ("ملف واحد",
    "un fichier"), so a branch may carry fewer `#` than its English
    counterpart. Every other placeholder is an argument the message promises
    to substitute, and must match exactly.

    A value that isn't a single top-level plural construct in both languages
    falls back to flat placeholder-multiset equality, same as the other
    catalogs."""
    en_branches = _parse_top_level_plural(en_value)
    target_branches = _parse_top_level_plural(target_value)
    if en_branches is None or target_branches is None:
        return placeholders_match(en_value, target_value, plural_leaf)
    if "other" not in en_branches:
        return False
    for label, tokens in target_branches.items():
        en_tokens = en_branches.get(label, en_branches["other"])
        if [t for t in tokens if t != "#"] != [t for t in en_tokens if t != "#"]:
            return False
        if tokens.count("#") > en_tokens.count("#"):
            return False
    return True


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

NON_ASCII_WORD_RE = re.compile(r"\b\w*[^\x00-\x7f]\w*(ing|ed|s)\b", re.UNICODE)
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


# ── .NET ResX ────────────────────────────────────────────────────────────────


def resx_leaves(en_path, target_path, locale):
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
    # strip the keywords before scanning ResX values (a measured
    # false-positive source in the C# catalogs).
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
    reports (Android/ResX), and whether to print band_hits at all (only
    under --verbose).
    """
    detector_hits = []
    band_hits = []
    for key, form, en_value, values in leaves:
        for locale, target_value in values.items():
            scan_value = strip_icu(target_value)
            reasons = []
            glued = glued_morphology_hits(scan_value, locale)
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


def placeholder_mismatches(leaves, multiset_fn=placeholder_multiset, equal_fn=None):
    if equal_fn is None:
        equal_fn = placeholders_match
    mismatches = []
    for key, form, en_value, values in leaves:
        en_multiset = multiset_fn(en_value)
        for locale, target_value in values.items():
            target_multiset = multiset_fn(target_value)
            if not equal_fn(en_value, target_value, form is not None):
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

    # ── Strict: the two xcstrings catalogs and the Android catalog ──────────
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

    android_leaves_all = []
    for loc, target_path in ANDROID_STRINGS.items():
        en_path = "bae-android/app/src/main/res/values/strings.xml"
        android_leaves_all.extend(android_leaves(en_path, target_path, loc))
    if android_leaves_all:
        detector_hits, band_hits = scan_leaves(android_leaves_all)
        unallowed = print_hits("Android strings.xml (strict)", detector_hits, allowed)
        total_gating_failures += len(unallowed)

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

    resx_leaves_all = []
    for loc, target_path in RESX_CHROME.items():
        resx_leaves_all.extend(resx_leaves(RESX_CHROME_EN, target_path, loc))
    if resx_leaves_all:
        detector_hits, band_hits = scan_leaves(resx_leaves_all)
        unallowed = print_hits("ResX chrome (strict)", detector_hits, allowed)
        total_gating_failures += len(unallowed)

        mismatches = placeholder_mismatches(resx_leaves_all, mf1_placeholder_multiset, mf1_placeholders_match)
        if mismatches:
            print(f"=== ResX chrome: {len(mismatches)} placeholder-multiset mismatch(es) ===")
            for m in mismatches:
                loc_label = m["key"] if not m["form"] else f"{m['key']} [{m['form']}]"
                print(f"  [{m['locale']}] {loc_label!r}")
                print(f"      en:  {m['en']!r} -> {m['en_placeholders']}")
                print(f"      val: {m['value']!r} -> {m['value_placeholders']}")
            total_gating_failures += len(mismatches)

        if verbose:
            print_hits("ResX chrome (report-only band)", band_hits, allowed)

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
