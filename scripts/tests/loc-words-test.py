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


class CloudTest(unittest.TestCase):
    def test_cloud_is_refused_in_every_language(self):
        self.assertEqual(check.cloud_words("Move to cloud"), ["cloud"])
        self.assertEqual(check.cloud_words("In die Cloud verschieben", "de"), ["Cloud"])
        self.assertEqual(check.cloud_words("Flyt til skyen", "da"), ["skyen"])
        self.assertEqual(check.cloud_words("Mover a la nube", "es"), ["nube"])
        self.assertEqual(check.cloud_words("העבר לענן", "he"), ["לענן"])
        self.assertEqual(check.cloud_words("نقل إلى السحابة", "ar"), ["السحاب"])
        self.assertEqual(check.cloud_words("クラウドに移動", "ja"), ["クラウド"])

    def test_storage_and_names_pass(self):
        self.assertEqual(check.cloud_words("Move to storage"), [])
        self.assertEqual(check.cloud_words("Restore from iCloud Keychain"), [])
        self.assertEqual(check.cloud_words("Backblaze, Cloudflare R2"), [])
        self.assertEqual(check.cloud_words("Kopibeskyttet", "da"), [])
        self.assertEqual(check.cloud_words("רענן", "he"), [])
        self.assertEqual(check.cloud_words("Felhasználónév", "hu"), [])
        self.assertEqual(check.cloud_words("America de Nord", "ro"), [])


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
