#!/usr/bin/env python3
"""Refuse on-screen words spec/words.md rules out.

Capitalization: every string bae puts on screen is in sentence case: the
first word of each sentence and proper names are capitalized, nothing else.
The macOS menu bar's menus and items are the one exception, in title case as
every Mac app's are.

Checks, over the macOS String Catalog, Android's strings.xml and core's
catalog (bae-bridge/loc/catalog.toml):

  - English: a word that starts a sentence, a quotation, a bracket, a plural
    branch or a step of a "Settings › Playback" path may be capitalized; past
    that, a capitalized word must be a name (NAMES), an acronym or a word
    written with a capital inside it ("iCloud", "MusicBrainz"), a button
    named by its icon's name ("Press Play"), or start another of the app's
    commands named in the sentence ("shown by Add device…"). The menu bar's
    own strings are left out.
  - The menu bar: each item and menu title the menu builders in
    bae-macos/bae/bae/Shell name is in title case: every word capitalized but
    the short joining words inside a title, and "bae".
  - Every other locale whose language capitalizes, but German, whose nouns are
    capitalized wherever they stand: a value of two or more words with every
    word capitalized, past names (with their endings: "Finderissa"), acronyms
    and abbreviations ("No."), is title case copied from English. German is
    left to its own grammar.

Exits non-zero on any hit, printing each.
"""
import json
import pathlib
import re
import sys
import tomllib
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
MAC_CATALOG = ROOT / "bae-macos/bae/bae/Localizable.xcstrings"
ANDROID_RES = ROOT / "bae-android/app/src/main/res"
CORE_CATALOG = ROOT / "bae-bridge/loc/catalog.toml"
MENU_SOURCES = [
    ROOT / "bae-macos/bae/bae/Shell" / name
    for name in (
        "MainMenu.swift",
        "MainMenu+Edit.swift",
        "MainMenu+Playback.swift",
        "MainMenu+View.swift",
        "OpenLibraryMenu.swift",
    )
]

# Names, and the words of names, that keep their capital mid-sentence:
# products, services, companies, MusicBrainz's Disc ID, and Apple's names for
# its own places ("System Settings", "iCloud Keychain"; Czech's "Klíčenka").
NAMES = {
    "Account", "AirPlay", "Amaray", "Amazon", "Android", "Apple", "Backblaze",
    "Cast", "Cloudflare", "Disc", "Discogs", "Dropbox", "Finder", "Google",
    "Keychain", "Klíčenk", "Mac", "Microsoft", "MusicBrainz", "OneDrive",
    "Settings", "Subsonic", "System",
}
# These words are names only inside these phrases: alone they are ordinary
# words.
NAME_PHRASES = ("System Settings", "iCloud Keychain", "Apple Account", "Disc ID")
PHRASE_ONLY = {"Settings", "System", "Account", "Keychain", "Disc"}

# Buttons a string names whose label is an icon, so no catalog value holds
# their name: "Press Play to continue".
NAMED_CONTROLS = {"Play"}

# Labels whose translations name a queue section by its heading, which keeps
# its capital there ("Ryd Næste": clear the section "Næste").
HEADING_REFERENCES = {
    "Clear up next",
    "Clear playing from",
    "Go to now playing",
    "queue_clear_up_next",
    "queue_clear_playing_from",
}

# core's pressing regions are place names.
CORE_PLACE_NAMES = "core.pressing.region."

# Short words a title keeps lowercase inside it (Apple's style).
TITLE_SMALL_WORDS = {
    "a", "an", "and", "as", "at", "but", "by", "for", "from", "in", "into",
    "nor", "of", "on", "or", "the", "to", "via", "with",
}

# Languages without letter case: nothing to check.
UNCASED = {"ar", "he", "ja", "ko", "zh-Hans", "zh-Hant"}

WORD_RE = re.compile(r"[^\W\d_][\w'’-]*")
PLACEHOLDER_RE = re.compile(r"%\d+\$(?:lld|[a-zA-Z@])|%lld|%@|%[sd]|\{[\w]+\}")
URL_RE = re.compile(r"\(?https?://\S+")
# What may come right before a capitalized word that starts something.
OPENERS = re.compile(r"(^|[.?!:]\s+|[“\"«»„‘(\[{›‹\n]\s*|\\n\s*)$")


def acronym_or_camel(word):
    """An acronym ("CD", "S3"), a word with a capital inside ("iCloud"), one
    letter of a range ("A–Z"), or a word with a digit in it."""
    return (
        len(word) == 1
        or any(c.isupper() for c in word[1:])
        or any(c.isdigit() for c in word)
    )


def is_name(word, text, start, inflected=False):
    """Whether `word`, at `start` in `text`, is a name or part of one. In
    another language a name may carry an ending ("Finderissa"); in any it may
    carry a suffix ("Mac-datorn", "Discogs'")."""
    if word in PHRASE_ONLY:
        return any(
            begin <= start < begin + len(phrase)
            for phrase in NAME_PHRASES
            for begin in (m.start() for m in re.finditer(re.escape(phrase), text))
        )
    names = NAMES - PHRASE_ONLY
    if inflected:
        return any(word.startswith(name) for name in names)
    return re.split(r"[-'’]", word)[0] in names


def mid_sentence_capitals(text, labels):
    """The capitalized words of `text` that start nothing and name nothing."""
    text = URL_RE.sub(" ", PLACEHOLDER_RE.sub("x", text))
    hits = []
    for match in WORD_RE.finditer(text):
        word = match.group(0)
        if not word[0].isupper() or acronym_or_camel(word):
            continue
        before = text[: match.start()]
        if not before.strip() or OPENERS.search(before):
            continue
        after = text[match.end() :]
        if after.startswith(" ›"):
            continue
        if is_name(word, text, match.start()) or word in NAMED_CONTROLS:
            continue
        if any(text.startswith(label, match.start()) for label in labels):
            continue
        hits.append(word)
    return hits


def title_case_copy(text):
    """Whether `text` is title case: two or more words past its names and
    abbreviations ("No."), every one capitalized."""
    text = URL_RE.sub(" ", PLACEHOLDER_RE.sub(" ", text))
    words = []
    for match in WORD_RE.finditer(text):
        word = match.group(0)
        if acronym_or_camel(word) or is_name(word, text, match.start(), True):
            continue
        if len(word) <= 3 and text[match.end() : match.end() + 1] == ".":
            continue
        words.append(word)
    return len(words) >= 2 and all(w[0].isupper() for w in words)


def title_case_misses(title):
    words = WORD_RE.findall(title)
    return [
        w
        for i, w in enumerate(words)
        if w[0].islower()
        and w != "bae"
        and (i in (0, len(words) - 1) or w not in TITLE_SMALL_WORDS)
    ]


def menu_titles():
    """The literal titles the menu builders pass to `item`, `menu` and
    `submenu`, and to `String(localized:)` there."""
    titles = set()
    call = re.compile(
        r"""(?:\bitem|\bmenu|\bsubmenu|String\(localized:)\(?\s*"((?:[^"\\]|\\.)+)\"""",
        re.S,
    )
    for source in MENU_SOURCES:
        titles.update(call.findall(source.read_text()))
    return titles


def mac_units(localization, form=None):
    unit = localization.get("stringUnit")
    if unit:
        yield form, unit.get("value", "")
    for variation in localization.get("variations", {}).values():
        for name, branch in variation.items():
            yield from mac_units(branch, name)


def android_values(path):
    for element in ET.parse(path).getroot():
        if element.get("translatable") == "false":
            continue
        if element.tag == "string":
            yield element.get("name"), "".join(element.itertext())
        elif element.tag == "plurals":
            for item in element:
                yield f"{element.get('name')}:{item.get('quantity')}", "".join(
                    item.itertext()
                )


def android_locale(directory):
    qualifier = directory.name[len("values-") :]
    if qualifier.startswith("b+"):
        return qualifier[2:].replace("+", "-")
    return qualifier


def command_labels(values):
    """The labels another string may name by their words: commands of two or
    more words, or ending in "…" ("Save as…", "Add device…")."""
    return {
        v
        for v in values
        if len(v) <= 40 and v[:1].isupper() and (" " in v or v.endswith("…"))
    }


def main():
    hits = []
    menu = menu_titles()
    mac = json.loads(MAC_CATALOG.read_text())["strings"]
    core = tomllib.loads(CORE_CATALOG.read_text())["messages"]

    # (where, key, English value) and, per locale, (where, key, value): every
    # string but the menu bar's and core's place names.
    english = []
    translated = {}
    for key, entry in mac.items():
        if key in menu:
            continue
        localizations = entry.get("localizations", {})
        if "en" in localizations:
            english += [("macOS", key, v) for _, v in mac_units(localizations["en"])]
        else:
            english.append(("macOS", key, key))
        for locale, localization in localizations.items():
            translated.setdefault(locale, []).extend(
                ("macOS", key, v) for _, v in mac_units(localization)
            )
    for directory in sorted(ANDROID_RES.glob("values*")):
        path = directory / "strings.xml"
        if not path.exists():
            continue
        values = [("Android", n, v) for n, v in android_values(path)]
        if directory.name == "values":
            english += values
        else:
            translated.setdefault(android_locale(directory), []).extend(values)
    for key, message in core.items():
        if key.startswith(CORE_PLACE_NAMES):
            continue
        english.append(("core", key, message["value"]))
        for locale, value in message.get("translations", {}).items():
            translated.setdefault(locale, []).append(("core", key, value))

    for title in sorted(menu):
        missed = title_case_misses(title)
        if missed:
            hits.append(f"menu bar: {title!r} is not title case ({', '.join(missed)})")

    labels = command_labels(v for _, _, v in english)
    for where, key, value in english:
        words = mid_sentence_capitals(value, labels - {value})
        if words:
            hits.append(f"{where}: {key!r} is {value!r}: {', '.join(words)}")

    for locale, values in sorted(translated.items()):
        if locale in UNCASED or locale in ("en", "de"):
            continue
        for where, key, value in values:
            if key in HEADING_REFERENCES:
                continue
            if title_case_copy(value):
                hits.append(f"{where} {locale}: {key!r} is {value!r}")

    for hit in hits:
        print(hit)
    if hits:
        print(
            f"\n{len(hits)} strings break spec/words.md \"Capitalization\": "
            "sentence case everywhere but the macOS menu bar, title case there.",
            file=sys.stderr,
        )
    return 1 if hits else 0


if __name__ == "__main__":
    sys.exit(main())
