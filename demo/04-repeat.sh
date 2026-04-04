#!/bin/bash
source "$(dirname "$0")/lib.sh"; setup_repo
type_cmd "git vanity repeat:3"
cleanup
