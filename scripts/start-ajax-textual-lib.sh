#!/usr/bin/env bash

ajax_binary_needs_build() {
  local ajax_bin="$1"
  local repo_root="$2"

  if [[ ! -x "${ajax_bin}" ]]; then
    return 0
  fi

  local source
  while IFS= read -r -d '' source; do
    if [[ "${source}" -nt "${ajax_bin}" ]]; then
      return 0
    fi
  done < <(
    find "${repo_root}" \
      \( -path "${repo_root}/target" -o -path "${repo_root}/.git" \) -prune \
      -o \( -name 'Cargo.toml' -o -name 'Cargo.lock' -o -name '*.rs' \) -print0
  )

  return 1
}
