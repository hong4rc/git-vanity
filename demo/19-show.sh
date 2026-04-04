#!/bin/bash
source "$(dirname "$0")/lib.sh"; setup_repo
echo -ne "\033[1;32m$\033[0m git vanity cafe\n"
git-vanity cafe; sleep 1
type_cmd "git vanity show"
cleanup
