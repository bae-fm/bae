#!/usr/bin/env bash
# Runs unit test classes, each in the module whose src/test holds it, in one
# Gradle run: `bae-android/test-class.sh [--print] <Class>...`. --print shows
# the Gradle command instead of running it.
#
# Each Gradle run spends about 20 seconds starting before any test runs, so
# `builds app-build` passes all of a build's classes here at once. Gradle runs
# each module's test task with a --tests filter per class, and --continue lets
# the other modules' tasks run after one fails, so Gradle's output names every
# class with a failing test.
set -euo pipefail

usage() {
  echo "usage: test-class.sh [--print] <Class>..." >&2
  exit 2
}

print=0
if [ "${1:-}" = "--print" ]; then
  print=1
  shift
fi
if [ $# -eq 0 ]; then
  usage
fi
for class in "$@"; do
  if [[ "$class" == -* ]]; then
    usage
  fi
done

cd "$(dirname "$0")"

# The module of each class, in the order the classes are named.
class_modules=()
for class in "$@"; do
  found=()
  while IFS= read -r file; do
    # <module>/src/test/...
    found+=("${file%%/src/test/*}")
  done < <(find . -path ./build -prune -o -path '*/build' -prune -o -path '*/src/test/*' -name "$class.kt" -print | sed 's|^\./||')
  if [ ${#found[@]} -eq 0 ]; then
    echo "test-class.sh: no module has $class.kt under src/test" >&2
    exit 1
  fi
  if [ ${#found[@]} -gt 1 ]; then
    echo "test-class.sh: $class.kt is in more than one module: ${found[*]}" >&2
    exit 1
  fi
  class_modules+=("${found[0]}")
done

# Each module's test task once, in the order of its first class, followed by
# the filters of its classes. macOS's bash 3.2 has no associative arrays, so
# the modules already added are a space-separated list.
args=(--continue)
added=" "
for module in "${class_modules[@]}"; do
  case "$added" in
  *" $module "*) continue ;;
  esac
  added="$added$module "
  args+=(":${module//\//:}:testFullDebugUnitTest")
  at=0
  for class in "$@"; do
    if [ "${class_modules[$at]}" = "$module" ]; then
      args+=(--tests "*$class")
    fi
    at=$((at + 1))
  done
done

if [ $print -eq 1 ]; then
  echo ./gradle.sh "${args[@]}"
  exit 0
fi
exec ./gradle.sh "${args[@]}"
