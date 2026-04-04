#!/bin/bash
type_cmd() {
    echo -ne "\033[1;32m$\033[0m "
    for ((i=0; i<${#1}; i++)); do echo -n "${1:$i:1}"; sleep 0.04; done
    sleep 0.3; echo; eval "$1" 2>&1; sleep 5
}
setup_repo() {
    REPO=$(mktemp -d); cd "$REPO"; git init -q
    git commit --allow-empty -m "feat: add user login" -q
}
setup_repo_multi() {
    REPO=$(mktemp -d); cd "$REPO"; git init -q
    git commit --allow-empty -m "feat: add login" -q; git-vanity cafe -q
    git commit --allow-empty -m "fix: auth bug" -q; git-vanity dead -q
    git commit --allow-empty -m "docs: readme" -q
}
cleanup() { rm -rf "$REPO"; }
