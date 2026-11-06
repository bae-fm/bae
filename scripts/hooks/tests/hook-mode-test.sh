#!/bin/bash

set -e

source "$(dirname "$0")/../lib/hook-mode.sh"

# The machine-wide setting, through a stand-in `builds`.
fake=$(mktemp -d)
trap 'rm -rf "$fake"' EXIT
write_builds() {
    printf '#!/bin/bash\n%s\n' "$1" > "$fake/builds"
    chmod +x "$fake/builds"
}
expect_mode() {
    local want="$1" got
    got=$(unset HOOK_MODE; PATH="$fake:$PATH" hook_mode 2> /dev/null) || got="failed"
    if [ "$got" != "$want" ]; then
        echo "with $2, the hook ran as '$got', not $want"
        exit 1
    fi
}
write_builds 'echo quick'
expect_mode quick "builds mode quick"
write_builds 'echo strict'
expect_mode strict "builds mode strict"
write_builds 'echo "unreadable" >&2; exit 1'
expect_mode strict "builds mode failing"
write_builds 'echo bogus'
expect_mode failed "builds mode printing a bogus mode"
rm "$fake/builds"
mode=$(unset HOOK_MODE; PATH="$fake:/usr/bin:/bin" hook_mode)
if [ "$mode" != strict ]; then
    echo "without builds, the hook ran as '$mode', not strict"
    exit 1
fi

# HOOK_MODE overrides the setting.
write_builds 'echo quick'
mode=$(HOOK_MODE=strict PATH="$fake:$PATH" hook_mode)
if [ "$mode" != strict ]; then
    echo "HOOK_MODE=strict lost to builds mode: '$mode'"
    exit 1
fi

mode=$(HOOK_MODE=strict hook_mode)
if [ "$mode" != strict ]; then
    echo "HOOK_MODE=strict ran as '$mode'"
    exit 1
fi

mode=$(HOOK_MODE=quick hook_mode)
if [ "$mode" != quick ]; then
    echo "HOOK_MODE=quick ran as '$mode'"
    exit 1
fi

for bad in Quick fast "quick "; do
    if (HOOK_MODE="$bad" hook_mode) > /dev/null 2>&1; then
        echo "HOOK_MODE='$bad' was accepted as a mode"
        exit 1
    fi
done

# The hook routes the heavy compile steps by mode: quick runs cargo check and
# skips clippy, rustdoc and the mobile check; strict runs them.
hook="$(dirname "$0")/../pre-commit"
for step in '(check --all-targets)' '(clippy --all-targets)' '(doc --no-deps)' '(clippy --lib --target aarch64-apple-ios)'; do
    if ! grep -qF "$step" "$hook"; then
        echo "the hook no longer builds $step"
        exit 1
    fi
done
if ! grep -q 'MODE" = strict' "$hook"; then
    echo "rustdoc and the mobile check are not gated on strict mode"
    exit 1
fi
