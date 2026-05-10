#!/usr/bin/env bash
# tests: osucollector.com API - GET /api/collections/{id}
# rust source: src/core/collection/api_client.rs
# model: Collection { id, name, uploader: { id, username }, beatmapsets: [{ id, beatmaps: [{ id, checksum }] }] }

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COLLECTION_ID=50
URL="https://osucollector.com/api/collections/$COLLECTION_ID"

echo "=== osucollector API: GET $URL ==="

resp=$(curl -s -w "\n%{http_code}" -H "User-Agent: $UA" "$URL")
status=$(echo "$resp" | tail -1)
body=$(echo "$resp" | head -n -1)

check_http_status "$status" 200 "osucollector collection fetch" || { echo "$body" | head -5; exit 1; }

check_field "$body" "id" "number" "collection.id"
check_field "$body" "name" "string" "collection.name"
check_field "$body" "beatmapsets" "array" "collection.beatmapsets"

uploader_id=$(echo "$body" | jq -r '.uploader.id // "__missing__"')
uploader_name=$(echo "$body" | jq -r '.uploader.username // "__missing__"')
if [[ "$uploader_id" == "__missing__" ]]; then
    fail "collection.uploader.id missing"
elif [[ "$uploader_name" == "__missing__" ]]; then
    fail "collection.uploader.username missing"
else
    pass "uploader shape: id=$uploader_id username=$uploader_name"
fi

beatmapset_count=$(echo "$body" | jq '.beatmapsets | length')
pass "beatmapsets array length: $beatmapset_count"

if [[ "$beatmapset_count" -gt 0 ]]; then
    first_bs=$(echo "$body" | jq '.beatmapsets[0]')
    check_field "$first_bs" "id" "number" "beatmapset[0].id"
    check_field "$first_bs" "beatmaps" "array" "beatmapset[0].beatmaps"

    beatmap_count=$(echo "$first_bs" | jq '.beatmaps | length')
    if [[ "$beatmap_count" -gt 0 ]]; then
        first_bm=$(echo "$first_bs" | jq '.beatmaps[0]')
        check_field "$first_bm" "id" "number" "beatmap[0].id"
        check_field "$first_bm" "checksum" "string" "beatmap[0].checksum"
        pass "beatmap shape ok: id=$(echo "$first_bm" | jq -r .id) checksum=$(echo "$first_bm" | jq -r .checksum | head -c 8)..."
    else
        pass "beatmapset[0] has no beatmaps (may be empty set)"
    fi
fi

echo ""
if [[ $FAILURES -eq 0 ]]; then
    echo "result: PASS ($(($(echo "$body" | jq '.beatmapsets | length'))) beatmapsets)"
else
    echo "result: FAIL ($FAILURES failure(s))"
    exit 1
fi
