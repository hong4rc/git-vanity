#!/bin/bash
source "$(dirname "$0")/lib.sh"; setup_repo
echo -ne "\033[1;32m$\033[0m git vanity cafe -n\n"
sleep 0.3
echo n | git-vanity cafe -n 2>&1
sleep 5
cleanup
