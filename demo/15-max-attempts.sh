#!/bin/bash
source "$(dirname "$0")/lib.sh"; setup_repo
type_cmd "git vanity 00000000 --max-attempts 1000"
cleanup
