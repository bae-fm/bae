#!/usr/bin/env bash
# Runs one unit test class in the module whose src/test holds it:
# `bae-android/test-class.sh [--print] <Class>`. --print shows the Gradle
# command instead of running it.
set -euo pipefail

print=0
if [ "${1:-}" = "--print" ]; then
  print=1
  shift
fi
if [ $# -ne 1 ] || [[ "$1" == -* ]]; then
  echo "usage: test-class.sh [--print] <Class>" >&2
  exit 2
fi
class=$1

cd "$(dirname "$0")"
modules=()
while IFS= read -r file; do
  # <module>/src/test/...
  modules+=("${file%%/src/test/*}")
done < <(find . -path ./build -prune -o -path '*/build' -prune -o -path '*/src/test/*' -name "$class.kt" -print | sed 's|^\./||')

if [ ${#modules[@]} -eq 0 ]; then
  echo "test-class.sh: no module has $class.kt under src/test" >&2
  exit 1
fi
if [ ${#modules[@]} -gt 1 ]; then
  echo "test-class.sh: $class.kt is in more than one module: ${modules[*]}" >&2
  exit 1
fi

task=":${modules[0]//\//:}:testFullDebugUnitTest"
if [ $print -eq 1 ]; then
  echo ./gradle.sh "$task" --tests "*$class"
  exit 0
fi
exec ./gradle.sh "$task" --tests "*$class"
