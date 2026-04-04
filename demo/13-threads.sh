#!/bin/bash
source "$(dirname "$0")/lib.sh"; setup_repo
type_cmd "git vanity cafe -j 2 -d"
cleanup
