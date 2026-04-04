#!/bin/bash
source "$(dirname "$0")/lib.sh"; setup_repo_multi
type_cmd "git vanity log"
cleanup
