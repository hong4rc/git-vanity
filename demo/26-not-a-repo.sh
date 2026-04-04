#!/bin/bash
source "$(dirname "$0")/lib.sh"
REPO=$(mktemp -d); cd "$REPO"
type_cmd "git vanity cafe"
cleanup
