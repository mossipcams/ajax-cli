#!/bin/sh
set -eu

chown -R ajax:ajax /ajax-dev

exec gosu ajax "$@"
