#!/usr/bin/env python3
"""Exercise the test-keychain check against the real sources and a planted
workspace holding each shape of test code it reads."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check-test-keychain.py"
# The check reads sources through the module beside it, which a script run
# from `scripts/` imports by name.
sys.path.insert(0, str(MODULE_PATH.parent))
SPEC = importlib.util.spec_from_file_location("check_test_keychain", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
CHECK = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECK
SPEC.loader.exec_module(CHECK)

PLANTED = {
    "Cargo.toml": '[workspace]\nmembers = ["planted", "bae-library"]\nresolver = "2"\n',
    "planted/Cargo.toml": (
        '[package]\nname = "planted"\nversion = "0.1.0"\nedition = "2024"\n\n'
        '[features]\ntest-utils = []\n'
    ),
    # The app's own code may name the person's keychain.
    "planted/src/lib.rs": """
pub enum Keychain { System, InMemory }
pub struct BridgeHost;
impl BridgeHost { pub fn new() -> Self { BridgeHost } }

pub fn app_keychain() -> Keychain { Keychain::System }

#[cfg(any(test, debug_assertions))]
pub fn debug_host() -> BridgeHost { BridgeHost::new() }

#[cfg(not(test))]
pub fn release_keychain() -> Keychain { Keychain::System }

pub fn keychain(test: bool) -> Keychain {
    match test {
        #[cfg(any(test, feature = "test-utils"))]
        true => Keychain::System,
        _ => Keychain::InMemory,
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn test_host<A, B>() -> BridgeHost {
    BridgeHost::new()
}

#[cfg(test)]
mod inline_tests {
    use super::Keychain::{InMemory, System};
    fn named_in_a_comment() {
        // Keychain::System
        let _ = "Keychain::System";
    }
}

mod child;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
""",
    "planted/src/child.rs": """
pub fn child_keychain() -> crate::Keychain { crate::Keychain::System }

#[cfg(test)]
mod grandchild;

mod inline {
    #![cfg(test)]
    fn reaches() { let _ = crate::Keychain::System; }
}
""",
    "planted/src/child/grandchild.rs": """
fn reaches() { let _ = crate::BridgeHost::new(); }
""",
    "planted/src/lib_tests.rs": """
mod nested;
include!("lib_tests/included.rs");
fn reaches() { let _ = crate::Keychain::System; }
""",
    # A file `#[path]` loads declares its modules as siblings, as mod.rs does.
    "planted/src/nested.rs": """
fn reaches() { let _ = crate::BridgeHost::new(); }
""",
    "planted/src/lib_tests/included.rs": """
fn included() { let _ = crate::Keychain::System; }
""",
    # A target's root declares its modules as siblings, whatever its name.
    "planted/tests/integration.rs": """
#[test]
fn reaches() { let _ = planted::Keychain::System; }
mod support;
""",
    "planted/tests/support.rs": """
pub fn reaches() { let _ = planted::BridgeHost::new(); }
""",
    "planted/benches/cost.rs": """
fn main() { let _ = planted::BridgeHost::new(); }
fn store(layout: Layout) { let _ = coven::Coven::builder(layout); }
fn home() { let _ = dirs::home_dir(); }
""",
    "planted/examples/probe.rs": """
fn main() { let _ = planted::Keychain::System; }
""",
    # The one example allowed to read the person's keychain and home, by its
    # path in bae; another example in its crate is not.
    "bae-library/Cargo.toml": (
        '[package]\nname = "bae-library"\nversion = "0.1.0"\nedition = "2024"\n'
    ),
    "bae-library/src/lib.rs": "",
    "bae-library/examples/restore_code.rs": """
fn main() { let home = dirs::home_dir(); let _ = coven::Coven::builder(layout(home)); }
""",
    "bae-library/examples/sync_bench.rs": """
fn main() { let _ = dirs::home_dir(); }
""",
    "app/mac/project.yml": """
name: app
targets:
  app:
    type: application
    sources:
      - path: app
  appTests:
    type: bundle.unit-test
    sources:
      - appTests
      - path: appTestSupport
schemes:
  app:
    build:
      targets:
        app: all
""",
    "app/mac/app/Host.swift": "let host = try BridgeHost(diagnostics: d, appDir: a)\n",
    "app/mac/appTests/HostTests.swift": (
        "let host = try BridgeHost(diagnostics: d, appDir: a)\n"
        "let fake = BridgeHost(unsafeFromHandle: handle)\n"
        "let made = try BridgeHost.withTestKeyring(diagnostics: d, appDir: a)\n"
    ),
    "app/mac/appTestSupport/Support.swift": (
        "let host = try BridgeHost.init(diagnostics: d, appDir: a)\n"
    ),
    "android/app/src/main/java/App.kt": "val host = BridgeHost(diagnostics, appDir, focus)\n",
    "android/app/src/test/java/HostTest.kt": (
        "val host = BridgeHost(diagnostics, appDir, focus)\n"
        "class FakeHost : BridgeHost(NoHandle)\n"
    ),
    "android/app/src/testFull/java/FullTest.kt": (
        "val host = BridgeHost(diagnostics, appDir, focus)\n"
    ),
    "desktop/Native.cs": "var host = new BridgeHost(diagnostics, appDir);\n",
    "desktop/Desktop.Tests/Desktop.Tests.csproj": (
        '<Project><ItemGroup><PackageReference Include="Microsoft.NET.Test.Sdk" />'
        "</ItemGroup></Project>\n"
    ),
    "desktop/Desktop.Tests/HostTests.cs": "var host = new BridgeHost(diagnostics, appDir);\n",
}


class TestKeychainCheckTests(unittest.TestCase):
    def test_production_tree_passes(self) -> None:
        self.assertEqual(CHECK.check(ROOT), [])

    def test_planted_test_code_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative, text in PLANTED.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            self.assertEqual(
                CHECK.check(root),
                [
                    "bae-library/examples/sync_bench.rs:2: dirs::home_dir",
                    "planted/benches/cost.rs:2: BridgeHost::new",
                    "planted/benches/cost.rs:3: Coven::builder",
                    "planted/benches/cost.rs:4: dirs::home_dir",
                    "planted/examples/probe.rs:2: Keychain::System",
                    "planted/src/child.rs:9: Keychain::System",
                    "planted/src/child/grandchild.rs:2: BridgeHost::new",
                    "planted/src/lib.rs:17: Keychain::System",
                    "planted/src/lib.rs:24: BridgeHost::new",
                    "planted/src/lib.rs:29: Keychain::System",
                    "planted/src/lib_tests.rs:4: Keychain::System",
                    "planted/src/lib_tests/included.rs:2: Keychain::System",
                    "planted/src/nested.rs:2: BridgeHost::new",
                    "planted/tests/integration.rs:3: Keychain::System",
                    "planted/tests/support.rs:2: BridgeHost::new",
                    "android/app/src/test/java/HostTest.kt:1: BridgeHost(...)",
                    "android/app/src/testFull/java/FullTest.kt:1: BridgeHost(...)",
                    "app/mac/appTestSupport/Support.swift:1: BridgeHost(...)",
                    "app/mac/appTests/HostTests.swift:1: BridgeHost(...)",
                    "desktop/Desktop.Tests/HostTests.cs:1: BridgeHost(...)",
                ],
            )


if __name__ == "__main__":
    unittest.main()
