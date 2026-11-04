#!/bin/bash
# rustc's linker signature does not bind Info.plist, so macOS treats each
# rebuild as a new unsigned app and CoreAudio rejects the microphone.
# Re-sign the final shiro binary with the bundle identifier from tauri.conf.
set -euo pipefail

args=()
for arg in "$@"; do
  if [[ "$arg" == @* && -f "${arg:1}" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
      args+=("$line")
    done < "${arg:1}"
  else
    args+=("$arg")
  fi
done

cc "$@"

out=""
prev=""
for arg in "${args[@]}"; do
  if [[ "$prev" == "-o" ]]; then
    out="$arg"
  fi
  prev="$arg"
done

# Cargo links the app to target/debug/deps/shiro-<hash>, then copies it to
# target/debug/shiro. Sign that deps executable so the copy keeps the identity.
base="$(basename "${out:-}")"
case "$base" in
  shiro|shiro-*)
    if [[ -f "$out" ]]; then
      codesign --force --sign - --identifier "com.adityaapkumar.shiro" "$out"
      echo "codesign: microphone permission identity bound to $out" >&2
    fi
    ;;
esac
