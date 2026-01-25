#!/bin/bash
# Watch the most recent release workflow run
RUN_ID=$(gh run list --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId')
if [[ -z "$RUN_ID" ]]; then
  echo "No release runs found"
  exit 1
fi
gh run watch "$RUN_ID" --exit-status
