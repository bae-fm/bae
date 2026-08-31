#!/bin/bash
set -euo pipefail

METADATA="$(cargo metadata --offline --format-version 1 --no-deps)"
export METADATA

python3 - <<'PY'
import json
import os

packages = {package["name"]: package for package in json.loads(os.environ["METADATA"])["packages"]}

generator = packages.get("bae-uniffi-bindgen")
if generator is None:
    raise SystemExit("host binding generator is not an isolated Cargo package")

generator_targets = {(target["name"], tuple(target["kind"])) for target in generator["targets"]}
if ("uniffi-bindgen", ("bin",)) not in generator_targets:
    raise SystemExit("bae-uniffi-bindgen does not produce the uniffi-bindgen host binary")

generator_dependencies = {dependency["name"] for dependency in generator["dependencies"]}
if generator_dependencies != {"uniffi"}:
    raise SystemExit(
        "host binding generator dependencies are not isolated: "
        + ", ".join(sorted(generator_dependencies))
    )

bridge = packages["bae-bridge"]
bridge_targets = {target["name"] for target in bridge["targets"]}
if "uniffi-bindgen" in bridge_targets:
    raise SystemExit("uniffi-bindgen still belongs to bae-bridge")

uniffi_dependency = next(
    dependency for dependency in bridge["dependencies"] if dependency["name"] == "uniffi"
)
if "cli" in uniffi_dependency["features"]:
    raise SystemExit("the mobile bridge target still compiles uniffi's host-only CLI")
PY

for build_script in bae-bridge/build-android.sh bae-bridge/build-ios.sh; do
    if ! grep -Fq -- '-p bae-uniffi-bindgen' "$build_script"; then
        echo "$build_script does not build the isolated host generator" >&2
        exit 1
    fi
    if ! grep -Fq -- '"$BINDGEN" generate' "$build_script"; then
        echo "$build_script does not execute the prebuilt host generator" >&2
        exit 1
    fi
    if ! grep -Fq -- 'BINDGEN="$CARGO_TARGET_DIR/debug/uniffi-bindgen"' "$build_script"; then
        echo "$build_script does not reuse the native debug generator" >&2
        exit 1
    fi
    if grep -Eq -- 'cargo run .*uniffi-bindgen|run_host_cargo run .*uniffi-bindgen' "$build_script"; then
        echo "$build_script still builds the bridge package for host binding generation" >&2
        exit 1
    fi
done

echo "mobile binding generation does not compile the bridge for the host"
