#!/usr/bin/env bash
# Verifies that an APK ships what the working tree actually says.
#
# Two failures this catches, both silent — the build reports success either way:
#
#   1. A stale asset reference. `asset!()` bakes a content hash into the binary,
#      but dx caches it; editing only `assets/app.css` does not invalidate that
#      cache, and neither does `touch src/main.rs` nor `cargo clean -p`. Since
#      the Gradle assets directory is never purged, the old file is still there
#      to be served, so instead of a 404 you get an app that works and looks
#      like an older version of itself. This happened for a full day of work.
#
#   2. More than one ABI. Successive builds leave their `libmain.so` behind in
#      the same output directory, so an APK can carry an arm64 binary from one
#      build and an x86_64 from another — and the phone picks the stale one.
#
# Usage:  tools/check-apk.sh [apk] [abi]
# Default: the release APK, arm64-v8a (the gérante's phone).
#
# `rm -rf target/dx` before a release build is what prevents both. This script
# is the proof that it worked.

set -euo pipefail

PROFILE="${PROFILE:-release}"
APK="${1:-target/dx/devis-mobile/$PROFILE/android/app/app/build/outputs/apk/$PROFILE/app-$PROFILE.apk}"
ABI="${2:-arm64-v8a}"
# Walked up to rather than counted out: a hard-coded number of `dirname` calls
# was off by one, and the `[ -f ]` that followed skipped the check in silence —
# which is the exact failure mode this script exists to catch.
find_manifest() {
    local dir
    dir=$(cd "$(dirname "$1")" && pwd)
    while [ "$dir" != / ]; do
        [ -f "$dir/.manifest.json" ] && { printf '%s' "$dir/.manifest.json"; return 0; }
        dir=$(dirname "$dir")
    done
    return 1
}

fail() { printf '\033[31m✗\033[0m %s\n' "$1" >&2; exit 1; }
pass() { printf '\033[32m✓\033[0m %s\n' "$1"; }

[ -f "$APK" ] || fail "APK introuvable : $APK"

# --- one ABI, and it is the one we target --------------------------------
# `|| true` on every grep: under `set -e` a no-match aborts the script mid-way,
# so the `fail` message below would never print — a guard that dies quietly is
# the exact thing this file exists to prevent.
libs=$(unzip -l "$APK" | grep -oE 'lib/[^/]+/libmain\.so' | sort -u || true)
[ -n "$libs" ] || fail "aucun binaire natif dans l'APK — build incomplet"

count=$(printf '%s\n' "$libs" | grep -c . || true)
if [ "$count" -ne 1 ]; then
    printf '%s\n' "$libs" >&2
    fail "$count binaires natifs dans l'APK — le téléphone peut charger le mauvais. Lancez : rm -rf target/dx"
fi
printf '%s' "$libs" | grep -q "lib/$ABI/" || fail "binaire pour $(dirname "$libs" | xargs basename), pas pour $ABI. Ajoutez --device au build."
pass "un seul binaire natif, pour $ABI"

# --- the binary asks for the asset the manifest actually bundled ---------
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
unzip -o -q "$APK" "lib/$ABI/libmain.so" -d "$work"

referenced=$(strings -a "$work/lib/$ABI/libmain.so" | grep -oE 'app-dxh[a-f0-9]+\.css' | sort -u || true)
[ -n "$referenced" ] || fail "aucune référence de feuille de style dans le binaire"
[ "$(printf '%s\n' "$referenced" | wc -l)" -eq 1 ] || fail "le binaire référence plusieurs feuilles : $referenced"

bundled=$(unzip -l "$APK" | grep -oE 'assets/app-dxh[a-f0-9]+\.css' | sed 's|assets/||' | sort -u || true)
[ -n "$bundled" ] || fail "aucune feuille de style dans l'APK"

[ "$(printf '%s\n' "$bundled" | wc -l)" -eq 1 ] || fail "$(printf '%s\n' "$bundled" | wc -l) feuilles de style dans l'APK — restes de builds précédents. Lancez : rm -rf target/dx"

[ "$referenced" = "$bundled" ] || fail "le binaire sert « $referenced » alors que l'APK embarque « $bundled ». Lancez : rm -rf target/dx"
pass "le binaire sert la feuille embarquée ($referenced)"

# --- and that asset is the file on disk right now ------------------------
MANIFEST=$(find_manifest "$APK") || fail "aucun .manifest.json au-dessus de l'APK — impossible de vérifier la feuille servie contre le disque"

expected=$(python3 -c "
import json, os
m = json.load(open('$MANIFEST'))
print(m['assets'][os.path.abspath('assets/app.css')][0]['bundled_path'])
") || fail "assets/app.css absent de $MANIFEST"

[ "$referenced" = "$expected" ] || fail "assets/app.css devrait donner « $expected », l'APK sert « $referenced ». Lancez : rm -rf target/dx"
pass "la feuille servie est bien assets/app.css tel qu'il est sur le disque"

printf '\n\033[32mAPK conforme.\033[0m %s\n' "$APK"
