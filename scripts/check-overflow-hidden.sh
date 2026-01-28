#!/bin/bash
# Check for overflow-hidden in UI components
# overflow-hidden creates a scroll container that blocks trackpad scroll propagation
# Use overflow-clip instead

if grep -rn "overflow-hidden" --include='*.rs' bae-ui/src/components/ 2>/dev/null | grep -v "//"; then
    echo "Found overflow-hidden in UI components. Use overflow-clip instead."
    exit 1
fi
