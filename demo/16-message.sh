#!/bin/bash
source "$(dirname "$0")/lib.sh"; setup_repo
echo -ne "\033[1;32m$\033[0m git vanity cafe --message \"new message\"\n"
git-vanity cafe --message "new message"
sleep 1
type_cmd "git log --oneline -1"
cleanup
