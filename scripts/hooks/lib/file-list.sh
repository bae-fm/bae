#!/bin/bash

staged_file_in_list() {
    local file="$1"
    grep -qxF "$file" <<< "$CHANGED_FILES"
}

staged_path_under() {
    local directory="${1%/}"
    while IFS= read -r file; do
        case "$file" in
            "$directory"/*) return 0 ;;
        esac
    done <<< "$CHANGED_FILES"
    return 1
}

staged_files_with_extension() {
    local extension="$1"
    while IFS= read -r file; do
        case "$file" in
            *."$extension") printf '%s\n' "$file" ;;
        esac
    done <<< "$CHANGED_FILES"
}

format_dotnet_whitespace() {
    local project="$1"
    shift
    dotnet format whitespace "$project" --include "$@"
}

changed_workspace_crates() {
    local crate
    for crate in "$@"; do
        if staged_path_under "$crate"; then
            printf '%s\n' "$crate"
        fi
    done
}

record_staged_worktree_state() {
    local state_file="$1"
    : > "$state_file"
    while IFS= read -r file; do
        [ -z "$file" ] && continue
        local had_unstaged=0
        if ! git diff --quiet -- "$file"; then
            had_unstaged=1
        fi
        printf '%s\t%s\t%s\n' \
            "$(worktree_file_hash "$file")" \
            "$had_unstaged" \
            "$file" >> "$state_file"
    done <<< "$CHANGED_FILES"
}

stage_formatter_changes() {
    local state_file="$1"
    local scope="${2:-}"
    local restaged=0
    local partial_changed=0
    while IFS=$'\t' read -r before_hash had_unstaged file; do
        [ -z "$file" ] && continue
        if [ -n "$scope" ]; then
            case "$file" in
                "$scope"*) ;;
                *) continue ;;
            esac
        fi
        if [ "$(worktree_file_hash "$file")" != "$before_hash" ]; then
            if [ "$had_unstaged" -eq 1 ]; then
                partial_changed=1
            else
                git add -- "$file"
                restaged=1
            fi
        fi
    done < "$state_file"
    if [ "$partial_changed" -eq 1 ]; then
        return 2
    fi
    if [ "$restaged" -eq 1 ]; then
        return 1
    fi
    return 0
}

worktree_file_hash() {
    local file="$1"
    if [ -e "$file" ] || [ -L "$file" ]; then
        git hash-object -- "$file"
    else
        printf 'missing'
    fi
}
