#!/bin/sh
set -eu

if [ "${AJAX_WEB_CHOWN_STATE:-1}" != "0" ]; then
    chown -R ajax:ajax /ajax-dev
fi

exec gosu ajax "$@"
