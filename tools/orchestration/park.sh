#!/usr/bin/env bash
# walgit collab watch --exec 的 hook：只落盘、返回 0（长活交给 dispatch）
Q=/e/tmp/openmir2-watch/queue; mkdir -p "$Q"
name="$(date +%s%3N)-$$"
cat > "$Q/$name.json"
{ for k in WALGIT_COLLAB_REF WALGIT_COLLAB_KIND WALGIT_COLLAB_THREAD WALGIT_COLLAB_ACTOR WALGIT_COLLAB_VERIFIED; do printf '%s=%s\n' "$k" "${!k:-}"; done; } > "$Q/$name.meta"
touch "$Q/pending.flag"
exit 0
