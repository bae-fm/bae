#!/usr/bin/env python3
"""Tests for scripts/loc-words.py's reading of a string's case and words."""

import importlib.util
import unittest
from pathlib import Path

CHECK = Path(__file__).resolve().parent.parent / "loc-words.py"
spec = importlib.util.spec_from_file_location("loc_words", CHECK)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)

LABELS = {"Save as…", "Add device…"}


def capitals(text):
    return check.mid_sentence_capitals(text, LABELS)


class EnglishTest(unittest.TestCase):
    def test_title_case_is_refused(self):
        self.assertEqual(capitals("Add to Queue"), ["Queue"])
        self.assertEqual(capitals("Clear Up Next"), ["Up", "Next"])
        self.assertEqual(capitals("Open Settings"), ["Settings"])

    def test_sentence_case_passes(self):
        self.assertEqual(capitals("Add to queue"), [])
        self.assertEqual(capitals("Folder name (A–Z)"), [])

    def test_a_sentence_quote_or_branch_starts_anew(self):
        self.assertEqual(capitals("Nothing ran. Fix what the error names."), [])
        self.assertEqual(capitals("Restore it with “Add device”."), [])
        self.assertEqual(capitals("{count, plural, one {# file left.} other {# files left.}}"), [])
        self.assertEqual(capitals("You can turn this back on in Settings › Playback."), [])

    def test_names_acronyms_and_camel_case_keep_their_capital(self):
        self.assertEqual(capitals("Reveal in Finder"), [])
        self.assertEqual(capitals("Sync via Microsoft OneDrive"), [])
        self.assertEqual(capitals("No LOG or CUE in the folder"), [])
        self.assertEqual(capitals("Restore from iCloud Keychain"), [])
        self.assertEqual(capitals("Enable it in System Settings, or paste"), [])
        self.assertEqual(capitals("Signed in to this Apple Account."), [])

    def test_settings_and_account_alone_are_ordinary_words(self):
        self.assertEqual(capitals("Open Settings"), ["Settings"])
        self.assertEqual(capitals("The Account is gone"), ["Account"])

    def test_a_command_named_in_a_sentence_keeps_its_capital(self):
        self.assertEqual(capitals("Formats appear in Save as… on tracks."), [])
        self.assertEqual(capitals("The code shown by Add device… on a device."), [])
        self.assertEqual(capitals("Formats appear in Save As… on tracks."), ["Save", "As"])


class ContractionTest(unittest.TestCase):
    def test_contractions_are_refused(self):
        self.assertEqual(check.contractions("Couldn’t pin"), ["Couldn’t"])
        self.assertEqual(check.contractions("It’s there; you’ll see"), ["It’s", "you’ll"])
        self.assertEqual(check.contractions("bae can't read it"), ["can't"])
        self.assertEqual(check.contractions("They’re here and I’m not"), ["They’re", "I’m"])

    def test_written_out_words_and_possessives_pass(self):
        self.assertEqual(check.contractions("Could not pin the library’s files"), [])
        self.assertEqual(check.contractions("This Mac’s copy; bae’s folder; Ana’s code"), [])


class LocaleTest(unittest.TestCase):
    def test_copied_title_case_is_refused(self):
        self.assertTrue(check.title_case_copy("Albümü Sil…"))
        self.assertTrue(check.title_case_copy("Kaynak Dosyayı Geri Yükle"))

    def test_sentence_case_and_names_pass(self):
        self.assertFalse(check.title_case_copy("Albümü sil…"))
        self.assertFalse(check.title_case_copy("Denne Macen"))
        self.assertFalse(check.title_case_copy("Näytä Finderissa"))
        self.assertFalse(check.title_case_copy("Katalog No."))
        self.assertFalse(check.title_case_copy("iCloud kullan"))


class MenuBarTest(unittest.TestCase):
    def test_title_case_passes(self):
        self.assertEqual(check.title_case_misses("Pause Between Sides and Discs"), [])
        self.assertEqual(check.title_case_misses("Check for Updates…"), [])
        self.assertEqual(check.title_case_misses("About bae"), [])

    def test_sentence_case_is_refused(self):
        self.assertEqual(check.title_case_misses("Restore on launch"), ["launch"])
        self.assertEqual(check.title_case_misses("and More"), ["and"])


if __name__ == "__main__":
    unittest.main()
