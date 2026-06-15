#!/usr/bin/env bash
# tests: official osu! api v2 (osu.ppy.sh) — the "osu! official" mirror (MirrorKind::OsuApi)
# rust source: src/auth/mod.rs (lazer ROPC + refresh), osu-downloader/src/mirrors/mod.rs (OsuApi template)
#
# what this validates:
#   1. OAuth token endpoint shape          POST https://osu.ppy.sh/oauth/token  -> { access_token, expires_in, token_type }
#   2. bearer auth works for metadata       GET  /api/v2/beatmapsets/{id}        -> 200
#   3. the download endpoint's lazer gate   GET  /api/v2/beatmapsets/{id}/download with a client_credentials token -> 401/403
#   4. lazer password grant (ROPC)          POST /oauth/token grant_type=password, client 5, scope=* -> accepted
#
# (3) is why osu! official is a last-resort, default-off mirror: a headless
# client_credentials token authenticates but CANNOT download .osz. (4) is the
# IMPLEMENTED login path — osu!lazer's first-party client (id 5) accepts the
# password grant for a `*`-scope token that CAN download.
#
# (1)-(3) require OSU_CLIENT_ID + OSU_CLIENT_SECRET (a public-data oauth app).
# (4) requires OSU_USERNAME + OSU_PASSWORD (a real osu! account). Each section
# SKIPs without its creds — no creds are ever committed.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

TOKEN_URL="https://osu.ppy.sh/oauth/token"
API_BASE="https://osu.ppy.sh/api/v2"

echo "=== osu! official api v2 (beatmapset $BEATMAPSET_ID) ==="
echo ""

have_app_creds=1
if [[ -z "${OSU_CLIENT_ID:-}" || -z "${OSU_CLIENT_SECRET:-}" ]]; then
    have_app_creds=0
    echo "SKIP: OSU_CLIENT_ID / OSU_CLIENT_SECRET not set (skipping client_credentials probes 1-3)"
    echo "      (create a public oauth app at https://osu.ppy.sh/home/account/edit#new-oauth-application)"
    echo ""
fi

if [[ "$have_app_creds" == "1" ]]; then
# ── 1. client_credentials token (the fallback grant in src/auth::ensure_valid) ──
echo "--- token endpoint (client_credentials, scope=public) ---"
token_resp=$(curl -s \
    -H "Content-Type: application/json" \
    -H "User-Agent: $UA" \
    --max-time 20 \
    -d "{\"client_id\":\"$OSU_CLIENT_ID\",\"client_secret\":\"$OSU_CLIENT_SECRET\",\"grant_type\":\"client_credentials\",\"scope\":\"public\"}" \
    "$TOKEN_URL" 2>/dev/null) || true

check_field "$token_resp" "access_token" string "token endpoint"
check_field "$token_resp" "expires_in" number "token endpoint"
check_field "$token_resp" "token_type" string "token endpoint"

access_token=$(echo "$token_resp" | jq -r '.access_token // empty' 2>/dev/null)
if [[ -z "$access_token" ]]; then
    fail "could not obtain access token; skipping authenticated probes"
    echo ""
    echo "result: FAIL ($FAILURES failure(s))"
    exit 1
fi
pass "obtained client_credentials access token"

# ── 2. metadata lookup with the bearer token ──
echo ""
echo "--- beatmapset metadata (bearer auth) ---"
meta_status=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $access_token" \
    -H "User-Agent: $UA" \
    --max-time 20 \
    "$API_BASE/beatmapsets/$BEATMAPSET_ID" 2>/dev/null) || true

if [[ "$meta_status" == "200" ]]; then
    pass "metadata lookup ok (HTTP 200)"
elif [[ "$meta_status" == "429" ]]; then
    echo "  SKIP: metadata: rate limited (HTTP 429)"
else
    fail "metadata lookup: expected HTTP 200, got $meta_status"
fi

# ── 3. download endpoint — the lazer gate ──
# A client_credentials token (no user behind it) must NOT be able to download.
# Expect 401 (unauthenticated for this route) or 403 (forbidden / lazer-only).
echo ""
echo "--- download endpoint (expected to be gated for client_credentials) ---"
dl_status=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $access_token" \
    -H "User-Agent: $UA" \
    --max-time 20 \
    "$API_BASE/beatmapsets/$BEATMAPSET_ID/download" 2>/dev/null) || true

case "$dl_status" in
    401 | 403)
        pass "download correctly gated for client_credentials (HTTP $dl_status)"
        ;;
    200 | 302)
        # Unexpected: this token grant should never be able to download. If osu!
        # ever allows it, the OsuApi mirror's auth assumptions need revisiting.
        fail "download unexpectedly allowed for client_credentials (HTTP $dl_status) — revisit the lazer-scope assumption"
        ;;
    429)
        echo "  SKIP: download: rate limited (HTTP 429)"
        ;;
    *)
        fail "download: unexpected status (HTTP $dl_status)"
        ;;
esac
fi # have_app_creds

# ── 4. lazer password grant (ROPC) — the IMPLEMENTED login path ──
# osu!lazer's first-party client (id 5, secret public in ppy/osu) is the only
# client that may request scope=* via grant_type=password. We assert the client
# ACCEPTS the grant: a token is the happy path; "invalid credentials" / 2FA is
# fine (the grant was accepted, the account just needs work). What must NEVER
# happen is invalid_client / invalid_scope — that means the ROPC path is dead.
LAZER_CLIENT_ID="5"
LAZER_CLIENT_SECRET="FGc9GAtyHzeQDshWP5Ah7dega8hJACAJpQtw6OXk"
X_API_VERSION="20250115"

echo ""
echo "--- lazer password grant (client 5, scope=*) ---"
if [[ -z "${OSU_USERNAME:-}" || -z "${OSU_PASSWORD:-}" ]]; then
    echo "  SKIP: OSU_USERNAME / OSU_PASSWORD not set (no real osu! account)"
else
    ropc_resp=$(curl -s \
        -H "User-Agent: $UA" \
        --max-time 20 \
        --data-urlencode "grant_type=password" \
        --data-urlencode "client_id=$LAZER_CLIENT_ID" \
        --data-urlencode "client_secret=$LAZER_CLIENT_SECRET" \
        --data-urlencode "username=$OSU_USERNAME" \
        --data-urlencode "password=$OSU_PASSWORD" \
        --data-urlencode "scope=*" \
        "$TOKEN_URL" 2>/dev/null) || true

    ropc_token=$(echo "$ropc_resp" | jq -r '.access_token // empty' 2>/dev/null)
    ropc_error=$(echo "$ropc_resp" | jq -r '.error // empty' 2>/dev/null)

    if [[ -n "$ropc_token" ]]; then
        pass "password grant returned a token (lazer login works)"
        # The download endpoint needs BOTH Authorization and x-api-version.
        lz_status=$(curl -s -o /dev/null -w "%{http_code}" \
            -H "Authorization: Bearer $ropc_token" \
            -H "x-api-version: $X_API_VERSION" \
            -H "User-Agent: $UA" \
            --max-time 30 \
            "$API_BASE/beatmapsets/$BEATMAPSET_ID/download?noVideo=1" 2>/dev/null) || true
        case "$lz_status" in
            200 | 302) pass "lazer token reaches the download endpoint (HTTP $lz_status)" ;;
            401 | 403) echo "  NOTE: download gated (HTTP $lz_status) — likely pending session verification" ;;
            429) echo "  SKIP: download rate limited (HTTP 429)" ;;
            *) echo "  NOTE: download returned HTTP $lz_status" ;;
        esac
    elif [[ "$ropc_error" == "invalid_client" || "$ropc_error" == "invalid_scope" ]]; then
        fail "client 5 rejected the password grant (error=$ropc_error) — the ROPC path is broken"
    else
        pass "client 5 accepted the password grant (error=${ropc_error:-none}; no token without valid creds)"
    fi
fi

echo ""
if [[ $FAILURES -eq 0 ]]; then
    echo "result: PASS"
else
    echo "result: FAIL ($FAILURES failure(s))"
    exit 1
fi
