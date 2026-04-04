#!/bin/bash
source "$(dirname "$0")/lib.sh"; setup_repo
echo -ne "\033[2m# normal (shows output)\033[0m\n"
sleep 0.5
echo -ne "\033[1;32m$\033[0m git vanity cafe\n"
git-vanity cafe; sleep 2
echo -ne "\033[2m# quiet (minimal output)\033[0m\n"
sleep 0.5
echo -ne "\033[1;32m$\033[0m git vanity cafe -q\n"
git-vanity cafe -q
sleep 5
cleanup
