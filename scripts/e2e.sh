#!/usr/bin/env bash
# End-to-end test of davra-hub + davra against a real Postgres.
# Needs: DATABASE_URL pointing at an empty database, and both binaries built.
#   BIN=target/debug DATABASE_URL=postgres://... scripts/e2e.sh
set -euo pipefail

BIN=${BIN:-target/debug}
HUB="$BIN/davra-hub"
CLI="$BIN/davra"
PORT=${PORT:-18080}
export DAVRA_HUB="http://127.0.0.1:$PORT"
export DAVRA_HOME="$(mktemp -d)"
# GitHub secrets are sealed at rest; every hub command below uses this key.
export DAVRA_MASTER_KEY=${DAVRA_MASTER_KEY:-$(openssl rand -hex 32)}
WORK="$(mktemp -d)"
pass=0

ok()   { pass=$((pass + 1)); echo "  ok  $*"; }
fail() { echo "  FAIL $*" >&2; exit 1; }
has()  { grep -qF -- "$2" <<<"$1" || fail "$3: expected '$2' in: $1"; }
hasnt(){ ! grep -qF -- "$2" <<<"$1" || fail "$3: did not expect '$2' in: $1"; }
as()   { local t=$1; shift; DAVRA_TOKEN="$t" "$CLI" "$@"; }

echo "== setup"
"$HUB" migrate >/dev/null
"$HUB" org create shaxsiy --name "Shaxsiy" >/dev/null
"$HUB" project create mosla --name "Mosla" >/dev/null
"$HUB" project create secret-proj >/dev/null
"$HUB" member add lionali --kind human --project mosla --role owner >/dev/null
"$HUB" member add gulom --kind human --tg-user 42 --project mosla --role maintainer >/dev/null
"$HUB" member add lionali-ai --kind agent --owner lionali --project mosla >/dev/null
"$HUB" member add gulom-ai --kind agent --owner gulom --project mosla >/dev/null
"$HUB" member add guest --kind human --project mosla --role viewer >/dev/null
T_LI=$("$HUB" token create lionali 2>/dev/null)
T_LAI=$("$HUB" token create lionali-ai --name laptop 2>/dev/null)
T_GAI=$("$HUB" token create gulom-ai 2>/dev/null)
T_GUEST=$("$HUB" token create guest 2>/dev/null)
ok "org, 2 projects, 5 members, 4 tokens"

cat >"$WORK/result.json" <<'JSON'
{ "name": "mosla · AI team", "type": "private_supergroup", "id": 1,
  "messages": [
    { "id": 1, "type": "service", "date": "2026-11-01T09:00:00", "text": "" },
    { "id": 2, "type": "message", "date": "2026-11-03T10:00:00", "date_unixtime": "1762164000",
      "from": "G'ulom", "from_id": "user42", "text": "ui-kit: Textarea maxLength endi soft limit bo'ladi, DOM'ga berilmaydi" },
    { "id": 3, "type": "message", "date": "2026-11-03T10:05:00", "from": "Ali", "from_id": "user7",
      "reply_to_message_id": 2, "text": ["IndustryStep uchun ", { "type": "bold", "text": "counter" }, " kerak"] },
    { "id": 4, "type": "message", "date": "2026-11-03T10:06:00", "from": "G'ulom", "from_id": "user42",
      "text": "DB parol: DB_PASSWORD=hunter2hunter2 (buni hech kim ko'rmasin)" }
  ] }
JSON
out=$("$HUB" import-telegram mosla "$WORK/result.json")
has "$out" "3 imported" "import"
out=$("$HUB" import-telegram mosla "$WORK/result.json")
has "$out" "0 imported" "re-import is idempotent"
ok "telegram export import (idempotent)"

"$HUB" serve --bind "127.0.0.1:$PORT" --no-migrate --cookie-secure false --client-ip-header cf-connecting-ip >"$WORK/hub.log" 2>&1 &
HUB_PID=$!
trap 'kill $HUB_PID 2>/dev/null || true; rm -rf "$DAVRA_HOME" "$WORK"' EXIT
for _ in $(seq 1 50); do curl -fs "$DAVRA_HUB/healthz" >/dev/null 2>&1 && break; sleep 0.1; done
curl -fs "$DAVRA_HUB/.well-known/davra.json" | grep -q '"protocol":1' || fail "well-known"
ok "hub up, /.well-known/davra.json"

echo "== auth"
code=$(curl -s -o /dev/null -w '%{http_code}' "$DAVRA_HUB/api/v1/me")
[ "$code" = 401 ] || fail "no token -> $code"
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer dvr_0000" "$DAVRA_HUB/api/v1/me")
[ "$code" = 401 ] || fail "bad token -> $code"
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $T_LAI" "$DAVRA_HUB/api/v1/projects/secret-proj/brief")
[ "$code" = 404 ] || fail "ungranted project -> $code"
ok "401 without/with bad token, 404 for an ungranted project"

out=$(as "$T_LAI" whoami)
has "$out" "@lionali-ai (agent)" "whoami"
has "$out" "owner: @lionali" "whoami owner"
ok "whoami"

echo "== secrets"
out=$(as "$T_LAI" search "DB_PASSWORD" --project mosla)
has "$out" "[REDACTED]" "imported secret redacted"
hasnt "$out" "hunter2" "imported secret redacted"
out=$(as "$T_LAI" post "deploy key: ghp_abcdefghijklmnopqrstuvwxyz0123456789 @gulom-ai" --project mosla 2>&1)
has "$out" "secret was removed" "post redaction notice"
ok "secrets redacted on import and on post"

echo "== handoff flow"
out=$(as "$T_LAI" post "@gulom-ai MOS-032: Textarea maxLength endi soft limit. IndustryStep.tsx:111 ga counter qo'shing, 500 dan oshsa canGenerate=false." --project mosla)
has "$out" "posted #" "post"
MSG_ID=$(grep -o '#[0-9]*' <<<"$out" | tr -d '#')
out=$(as "$T_GAI" inbox --project mosla)
has "$out" "MOS-032" "gulom-ai inbox has handoff"
has "$out" "[peer_agent]" "trust label"
out=$(as "$T_GAI" inbox --hook --project mosla </dev/null)
has "$out" "MOS-032" "hook inbox prints"
out=$(as "$T_GAI" inbox --hook --project mosla </dev/null)
[ -z "$out" ] || fail "hook inbox must be empty after delivery, got: $out"
ok "post -> addressed inbox -> hook delivers once, then adds nothing"

# A human reply is [owner] for their own agent and [peer_human] for the other.
curl -fs -X POST -H "Authorization: Bearer $T_LI" -H 'content-type: application/json' \
  -d "{\"text\":\"@lionali-ai @gulom-ai rahmat, review qilaman\",\"reply_to\":$MSG_ID}" \
  "$DAVRA_HUB/api/v1/projects/mosla/messages" >/dev/null
out=$(as "$T_LAI" inbox --project mosla)
has "$out" "lionali [owner]" "owner trust for own agent"
out=$(as "$T_GAI" inbox --project mosla)
has "$out" "lionali [peer_human]" "peer_human for other agent"
ok "trust is relative to the reader"

echo "== live inbox"
curl -fs -X POST -H "Authorization: Bearer $T_GAI" -H 'content-type: application/json' -d '{"up_to":1000000}' \
  "$DAVRA_HUB/api/v1/projects/mosla/ack" >/dev/null
start=$(date +%s)
curl -fs -H "Authorization: Bearer $T_GAI" "$DAVRA_HUB/api/v1/projects/mosla/inbox?wait=2" >"$WORK/empty.json"
[ $(( $(date +%s) - start )) -ge 2 ] || fail "an empty inbox should hold the request for wait seconds"
has "$(cat "$WORK/empty.json")" '"messages":[]' "empty after the wait"
curl -fs -H "Authorization: Bearer $T_GAI" "$DAVRA_HUB/api/v1/projects/mosla/inbox?wait=20" >"$WORK/live.json" &
LIVE_PID=$!
sleep 1
start=$(date +%s)
curl -fs -X POST -H "Authorization: Bearer $T_LI" -H 'content-type: application/json' \
  -d '{"text":"@gulom-ai live test"}' "$DAVRA_HUB/api/v1/projects/mosla/messages" >/dev/null
wait $LIVE_PID
[ $(( $(date +%s) - start )) -le 3 ] || fail "a waiting inbox must return as soon as a message lands"
has "$(cat "$WORK/live.json")" "live test" "the waiting request got the new message"
ok "inbox?wait holds an empty inbox and returns the moment a message arrives"

echo "== search"
out=$(as "$T_GAI" search "MOS-032" --project mosla)
has "$out" "MOS-032" "fts exact token"
out=$(as "$T_GAI" search "Textarea maxlenght" --project mosla)
has "$out" "Textarea" "trigram typo tolerance"
out=$(as "$T_GAI" search "counter" --project mosla)
has "$out" "IndustryStep" "imported rich text searchable"
ok "full-text + trigram search"

echo "== brief"
out=$(as "$T_GAI" brief --project mosla)
has "$out" "you are @gulom-ai" "brief header"
has "$out" "@gulom-ai (agent of @gulom)" "brief members"
has "$out" "Only messages marked [owner]" "trust preamble"
[ ${#out} -lt 6000 ] || fail "brief too long: ${#out}"
out=$(echo '{"cwd":"/nonexistent","hook_event_name":"SessionStart"}' | DAVRA_TOKEN=dvr_bad "$CLI" brief --hook; echo "rc=$?")
has "$out" "rc=0" "hook never fails the session"
out=$(printf '{"cwd":"%s"}' "$WORK" | DAVRA_TOKEN="$T_GAI" "$CLI" brief --hook)
[ -z "$out" ] || fail "hooks must stay silent outside a repo with .davra.toml, got: $out"
ok "brief within budget; hook exits 0 on errors and stays silent outside opted-in repos"

echo "== guards"
out=$(DAVRA_TOKEN="$T_GUEST" "$CLI" post "salom" --project mosla 2>&1 || true)
has "$out" "viewers cannot post" "viewer"
for i in 1 2; do as "$T_LAI" post "agent ping $i" --project mosla >/dev/null; as "$T_GAI" post "agent pong $i" --project mosla >/dev/null; done
out=$(as "$T_LAI" post "one more" --project mosla 2>&1 || true)
has "$out" "loop_guard" "loop guard"
as "$T_LI" post "to'xtanglar, keyinroq" --project mosla >/dev/null
out=$(as "$T_LAI" post "ok" --project mosla)
has "$out" "posted #" "human unblocks agents"
ok "viewer cannot post; loop guard stops agents until a human writes"

echo "== get_message"
body=$(curl -fs -H "Authorization: Bearer $T_GAI" "$DAVRA_HUB/api/v1/messages/$MSG_ID?context=1")
has "$body" "\"id\":$MSG_ID" "get_message"
ok "get_message with context"

echo "== handoffs"
"$HUB" member roles gulom-ai --set "core, auth" >/dev/null
"$HUB" member roles lionali-ai --set "ui-kit" >/dev/null
REPO="$WORK/repo"; mkdir -p "$REPO/docs/memory/handoffs"
printf 'project = "mosla"\nhandoffs = "docs/memory/handoffs"\n' > "$REPO/.davra.toml"
cat > "$REPO/docs/memory/handoffs/2026-10-30-db-to-core-MOS-018.md" <<'MD'
---
from: db
to: core
ticket: MOS-018
status: done        # open | done
created: 2026-10-30
---
Done: migrated
What changed: tenants table.
MD
cat > "$REPO/docs/memory/handoffs/2026-11-02-architect-to-core-auth-MOS-020.md" <<'MD'
---
from: architect
to: core, auth
ticket: MOS-020
status: open
created: 2026-11-02
---
What changed: ContactListResponse gained nextCursor.
What you need to do: switch to cursor paging.
Done when: second page loads.
MD
before=$(curl -fs -H "Authorization: Bearer $T_GAI" "$DAVRA_HUB/api/v1/projects/mosla/inbox" | { grep -o '"id":' || true; } | wc -l)
out=$(cd "$REPO" && DAVRA_TOKEN="$T_LAI" "$CLI" handoffs sync)
has "$out" "2 new" "first sync imports"
after=$(curl -fs -H "Authorization: Bearer $T_GAI" "$DAVRA_HUB/api/v1/projects/mosla/inbox" | { grep -o '"id":' || true; } | wc -l)
[ "$before" = "$after" ] || fail "first sync must be quiet ($before -> $after inbox items)"
out=$(cd "$REPO" && DAVRA_TOKEN="$T_LAI" "$CLI" handoffs sync)
has "$out" "0 new, 0 updated" "second sync sends nothing"
out=$(cd "$REPO" && DAVRA_TOKEN="$T_GAI" "$CLI" brief)
has "$out" "Open handoffs for you (1)" "role-addressed handoff in brief"
has "$out" "MOS-020 architect→core,auth" "handoff line"
has "$out" "Your roles: core, auth" "roles in brief"
ok "repo handoff files: quiet first import, cached re-sync, routed by role"

# lionali-ai (role ui-kit) creates one through MCP; it lands as a file + announcement.
mcp() { (printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}}'; sleep 0.4
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'; printf '%s\n' "$2"; sleep 2) \
  | (cd "$REPO" && DAVRA_TOKEN="$1" timeout 10 "$CLI" mcp 2>>"$WORK/mcp.err") || true; }
out=$(mcp "$T_LAI" '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"handoff_create","arguments":{"to":["core"],"ticket":"MOS-032","changed":"Textarea maxLength is a soft limit","todo":"add a counter in IndustryStep.tsx:111","done_when":"over 500 chars blocks generate"}}}')
has "$out" "created and announced to core" "handoff_create via MCP"
NEWFILE=$(ls "$REPO/docs/memory/handoffs" | grep ui-kit-to-core-MOS-032)
[ -n "$NEWFILE" ] || fail "handoff file not written"
grep -q "^from: ui-kit" "$REPO/docs/memory/handoffs/$NEWFILE" || fail "from should default to the sender's role"
HID=$(grep -o 'Handoff #[0-9]*' <<<"$out" | tr -dc 0-9)
out=$(as "$T_GAI" inbox --project mosla)
has "$out" "Handoff #$HID · MOS-032 · ui-kit → core" "recipient's inbox has the announcement"
ok "handoff_create writes a Mosla-format file and announces it to the role"

out=$(mcp "$T_GAI" "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"handoff_done\",\"arguments\":{\"id\":$HID,\"note\":\"counter added, tests green\"}}}")
has "$out" "now says status: done" "handoff_done via MCP"
grep -q "^status: done" "$REPO/docs/memory/handoffs/$NEWFILE" || fail "file not marked done"
grep -q "^Done: counter added, tests green" "$REPO/docs/memory/handoffs/$NEWFILE" || fail "done note missing"
out=$(as "$T_LAI" inbox --project mosla)
has "$out" "Done · handoff #$HID · MOS-032 · counter added, tests green" "sender is told"
out=$(as "$T_GAI" handoffs list --status done --project mosla)
has "$out" "#$HID MOS-032" "listed as done"
ok "handoff_done updates the file, closes it on the hub, tells the sender"

echo "== MCP over stdio"
mcp_out=$( (
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}}'
  sleep 0.5
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"history_search","arguments":{"query":"MOS-032","project":"mosla"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"team_brief","arguments":{"project":"mosla"}}}'
  sleep 2
) | DAVRA_TOKEN="$T_GAI" timeout 10 "$CLI" mcp 2>"$WORK/mcp.err" || true)
for tool in projects team_brief inbox ack post history_search get_message handoffs get_handoff handoff_create handoff_done ask_owner; do
  has "$mcp_out" "\"name\":\"$tool\"" "tools/list has $tool"
done
has "$mcp_out" "MOS-032" "tools/call history_search"
has "$mcp_out" "you are @gulom-ai" "tools/call team_brief"
has "$mcp_out" "Davra connects you" "server instructions"
ok "initialize, tools/list (12 tools), tools/call"
[[ "$mcp_out" != *'claude/channel'* ]] || fail "plain davra mcp must not claim to be a channel"

echo "== live channel"
ack_all() { curl -fs -X POST -H "Authorization: Bearer $1" -H 'content-type: application/json' -d '{"up_to":1000000}' \
  "$DAVRA_HUB/api/v1/projects/mosla/ack" >/dev/null; }
say() { curl -fs -X POST -H "Authorization: Bearer $T_LI" -H 'content-type: application/json' \
  -d "{\"text\":\"$1\"}" "$DAVRA_HUB/api/v1/projects/mosla/messages" >/dev/null; }
ack_all "$T_GAI"
ch_out=$( (
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}}'
  sleep 0.5
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  sleep 1.5
  say "@gulom-ai channel test"
  sleep 3
) | (cd "$REPO" && DAVRA_CHANNEL=1 DAVRA_TOKEN="$T_GAI" timeout 12 "$CLI" mcp 2>"$WORK/ch.err") || true)
has "$ch_out" '"claude/channel"' "channel capability declared"
has "$ch_out" '"method":"notifications/claude/channel"' "message pushed into the session"
has "$ch_out" 'channel test' "pushed content"
has "$(as "$T_GAI" inbox --project mosla)" "No unread" "pushed messages count as read"
# Stop hook: waiting messages keep a finished agent going, once in a row.
say "@gulom-ai stop hook test"
out=$(printf '{"cwd":"%s","stop_hook_active":false}' "$REPO" | DAVRA_TOKEN="$T_GAI" "$CLI" inbox --hook --stop)
has "$out" '"decision":"block"' "stop hook blocks with new messages"
has "$out" 'stop hook test' "stop hook hands the messages over"
out=$(printf '{"cwd":"%s","stop_hook_active":false}' "$REPO" | DAVRA_TOKEN="$T_GAI" "$CLI" inbox --hook --stop)
[ -z "$out" ] || fail "stop hook with nothing new must stay silent, got: $out"
say "@gulom-ai one more"
out=$(printf '{"cwd":"%s","stop_hook_active":true}' "$REPO" | DAVRA_TOKEN="$T_GAI" "$CLI" inbox --hook --stop)
[ -z "$out" ] || fail "stop hook must let a continued agent rest, got: $out"
ack_all "$T_GAI"
ok "davra mcp as a channel pushes new messages live; Stop hook delivers between turns without looping"

echo "== ask_owner"
out=$(mcp "$T_LAI" '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ask_owner","arguments":{"question":"MOS-032 ni main ga merge qilaymi?","options":["Ha, merge qil","Yo'\''q, kut"]}}}')
has "$out" "sent to @lionali" "ask_owner via MCP"
QID=$(grep -o 'Question #[0-9]*' <<<"$out" | head -1 | tr -dc 0-9)
out=$(as "$T_LI" inbox --project mosla)
has "$out" "Question #$QID for @lionali: MOS-032 ni main ga merge qilaymi?" "owner sees the question"
out=$(as "$T_GAI" answer "$QID" "ha" 2>&1 || true)
has "$out" "not found" "only the owner can answer"
as "$T_LI" answer "$QID" "Ha, merge qil" >/dev/null
out=$(as "$T_LAI" inbox --project mosla)
has "$out" "lionali [owner]" "answer arrives as owner"
has "$out" "Answer to question #$QID: Ha, merge qil" "answer text"
ok "ask_owner: question reaches the owner, only the owner answers, answer is [owner] for the agent"

echo "== github webhook"
GH_OUT=$("$HUB" project github mosla --repo acme/mosla --public-url "$DAVRA_HUB" 2>&1)
hasnt "$GH_OUT" "unencrypted" "a configured master key seals the secret"
GH_SECRET=$(awk '/Secret:/ {print $2}' <<<"$GH_OUT")
GH_URL="$DAVRA_HUB/webhooks/github/shaxsiy/mosla"
payload='{"ref":"refs/heads/main","pusher":{"name":"gulom"},"commits":[{}],"head_commit":{"message":"MOS-032: counter added"}}'
sig="sha256=$(printf '%s' "$payload" | openssl dgst -sha256 -hmac "$GH_SECRET" -hex | awk '{print $NF}')"
code=$(curl -s -o /dev/null -w '%{http_code}' -H 'X-GitHub-Event: push' -H "X-Hub-Signature-256: sha256=00" -d "$payload" "$GH_URL")
[ "$code" = 401 ] || fail "bad signature -> $code"
code=$(curl -s -o /dev/null -w '%{http_code}' -H 'X-GitHub-Event: push' -H "X-Hub-Signature-256: $sig" -H 'content-type: application/json' -d "$payload" "$GH_URL")
[ "$code" = 204 ] || fail "signed push -> $code"
out=$(as "$T_GAI" search "counter added" --project mosla)
has "$out" "github [external]: push to main by gulom: MOS-032: counter added" "push stored as external"
ok "github webhook: secret sealed at rest, bad signature 401, signed push stored as [external]"

echo "== web accounts"
J=(-H 'content-type: application/json')
CSRF=(-H 'X-Davra: 1')
JAR="$WORK/cookies.txt"
code=$(curl -s -o /dev/null -w '%{http_code}' "${J[@]}" -d '{"name":"Ali","email":"ali@qitmir.uz","password":"long-enough-pw"}' "$DAVRA_HUB/api/v1/auth/register")
[ "$code" = 403 ] || fail "register without X-Davra -> $code"
hdrs=$(curl -s -D - -o "$WORK/reg.json" -c "$JAR" "${J[@]}" "${CSRF[@]}" -d '{"name":"Ali","email":"Ali@Qitmir.uz","password":"long-enough-pw"}' "$DAVRA_HUB/api/v1/auth/register")
has "$hdrs" "HTTP/1.1 201" "register"
has "$hdrs" "HttpOnly; SameSite=Lax" "session cookie flags"
has "$(cat "$WORK/reg.json")" '"name":"Ali"' "register body"
body=$(curl -s "${J[@]}" "${CSRF[@]}" -d '{"name":"X","email":"ali@qitmir.uz","password":"long-enough-pw"}' "$DAVRA_HUB/api/v1/auth/register")
has "$body" '"error":"email_taken"' "e-mail unique, case-insensitive"
body=$(curl -s "${J[@]}" "${CSRF[@]}" -d '{"name":"X","email":"x@qitmir.uz","password":"short"}' "$DAVRA_HUB/api/v1/auth/register")
has "$body" '"error":"bad_request"' "short password"
body=$(curl -s -b "$JAR" "$DAVRA_HUB/api/v1/auth/session")
has "$body" '"email":"Ali@Qitmir.uz"' "session"
ok "register: CSRF header required, cookie HttpOnly+Lax, unique e-mail, password policy"

body=$(curl -s "${J[@]}" "${CSRF[@]}" -d '{"email":"ali@qitmir.uz","password":"wrong-password"}' "$DAVRA_HUB/api/v1/auth/login")
has "$body" '"error":"invalid_credentials"' "wrong password"
body=$(curl -s "${J[@]}" "${CSRF[@]}" -d '{"email":"nobody@qitmir.uz","password":"whatever-pw"}' "$DAVRA_HUB/api/v1/auth/login")
has "$body" '"error":"invalid_credentials"' "unknown e-mail looks the same"
body=$(curl -s -c "$JAR" "${J[@]}" "${CSRF[@]}" -d '{"email":"ALI@qitmir.uz","password":"long-enough-pw"}' "$DAVRA_HUB/api/v1/auth/login")
has "$body" '"name":"Ali"' "login"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$JAR" -c "$JAR" "${CSRF[@]}" -X POST "$DAVRA_HUB/api/v1/auth/logout")
[ "$code" = 204 ] || fail "logout -> $code"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$JAR" "$DAVRA_HUB/api/v1/auth/session")
[ "$code" = 401 ] || fail "session after logout -> $code"
for _ in $(seq 1 9); do curl -s -o /dev/null "${J[@]}" "${CSRF[@]}" -d '{"email":"ali@qitmir.uz","password":"guess-guess"}' "$DAVRA_HUB/api/v1/auth/login"; done
body=$(curl -s "${J[@]}" "${CSRF[@]}" -d '{"email":"ali@qitmir.uz","password":"guess-guess"}' "$DAVRA_HUB/api/v1/auth/login")
has "$body" '"error":"rate_limited"' "login rate limit"
ok "login: same error for wrong password and unknown e-mail, logout ends the session, rate limited"

body=$(curl -s -D - "$DAVRA_HUB/api/v1/nope")
has "$body" "404" "unknown api path"
has "$body" '"error":"not_found"' "json 404"
has "$body" "content-security-policy: default-src 'self'" "csp header"
has "$body" "x-content-type-options: nosniff" "nosniff header"
ok "unknown API paths are JSON 404s; security headers set"

echo "== web app"
WJ="$WORK/web.txt"
curl -s -c "$WJ" "${J[@]}" "${CSRF[@]}" -d '{"name":"Dilnoza","email":"dilnoza@qitmir.uz","password":"web-app-password"}' "$DAVRA_HUB/api/v1/auth/register" >/dev/null
W() { curl -s -b "$WJ" "$@"; }
WP() { local path=$1; shift; curl -s -b "$WJ" "${J[@]}" "${CSRF[@]}" -d "$1" "$DAVRA_HUB$path"; }
body=$(W "$DAVRA_HUB/api/v1/web/overview")
has "$body" '"projects":[]' "fresh account has no projects"
has "$body" '"linked":false' "telegram not linked"
body=$(WP /api/v1/web/projects '{"org":"shaxsiy","slug":"x","name":"X","handle":"dilnoza"}')
has "$body" '"field":"org"' "cannot join someone else's org by name"
body=$(WP /api/v1/web/projects '{"org":"dilnoza-lab","org_name":"Dilnoza Lab","slug":"crm","name":"CRM","handle":"dilnoza"}')
has "$body" '"slug":"crm"' "create project"
has "$body" '"role":"owner"' "creator is owner"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/messages '{"text":"salom jamoa, @dilnoza-ai bugun login formani qil"}')
has "$body" '"id":' "web post"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/agents '{"handle":"dilnoza-ai","roles":["core","auth"]}')
has "$body" '"handle":"dilnoza-ai"' "create agent"
has "$body" '"mine":true' "agent is mine"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/agents '{"handle":"dilnoza-ai"}')
has "$body" '"field":"handle"' "duplicate agent handle"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/agents/dilnoza-ai/tokens '{"name":"laptop"}')
has "$body" '"davra login --hub' "token with connect commands"
TOK=$(grep -o '"token":"dvr_[a-f0-9]*"' <<<"$body" | cut -d'"' -f4)
TID=$(grep -o '"id":"[0-9a-f-]*"' <<<"$body" | head -1 | cut -d'"' -f4)
out=$(DAVRA_TOKEN="$TOK" "$CLI" inbox --project crm)
has "$out" "dilnoza [owner]" "the new agent reads its owner's web message as [owner]"
body=$(W "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm")
has "$body" '"messages_7d":1' "stats"
has "$body" '"name":"laptop"' "my agent's tokens listed"
has "$body" '"roles":["core","auth"]' "agent roles"
body=$(W "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/search?q=login")
has "$body" '"author":"dilnoza"' "web search"
code=$(W -o /dev/null -w '%{http_code}' "$DAVRA_HUB/api/v1/web/projects/shaxsiy/mosla")
[ "$code" = 404 ] || fail "other people's projects must be invisible -> $code"
code=$(W -o /dev/null -w '%{http_code}' "${CSRF[@]}" -X POST "$DAVRA_HUB/api/v1/web/tokens/$TID/revoke")
[ "$code" = 204 ] || fail "revoke -> $code"
out=$(DAVRA_TOKEN="$TOK" "$CLI" whoami 2>&1 || true)
has "$out" "invalid token" "revoked token stops working"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$WJ" "${J[@]}" -d '{"text":"no csrf"}' "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/messages")
[ "$code" = 403 ] || fail "web POST without X-Davra -> $code"
ok "web app: create project, post, agents with roles, one-time token works then revokes, others' projects hidden, CSRF enforced"

echo "== team invites"
# New accounts come from another address so the register rate limit is not the subject here.
IP2=(-H 'cf-connecting-ip: 10.0.0.2')
code_of() { grep -o '"url":"[^"]*"' <<<"$1" | sed 's#.*/invite/##; s#"$##'; }
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/invites '{"role":"owner"}')
has "$body" '"field":"role"' "owners are not handed out by invite"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/invites '{"role":"member"}')
has "$body" "$DAVRA_HUB/invite/" "invite url"
INV=$(code_of "$body")
body=$(curl -s "$DAVRA_HUB/api/v1/web/invites/$INV")
has "$body" '"status":"open"' "public invite info"
has "$body" '"invited_by":"dilnoza"' "invite says who invited"
BJ="$WORK/bobur.txt"
curl -s -c "$BJ" "${J[@]}" "${CSRF[@]}" "${IP2[@]}" -d '{"name":"Bobur","email":"bobur@qitmir.uz","password":"bobur-password"}' "$DAVRA_HUB/api/v1/auth/register" >/dev/null
BP() { local path=$1; shift; curl -s -b "$BJ" "${J[@]}" "${CSRF[@]}" -d "$1" "$DAVRA_HUB$path"; }
body=$(BP "/api/v1/web/invites/$INV/accept" '{}')
has "$body" '"field":"handle"' "a new person picks a handle"
body=$(BP "/api/v1/web/invites/$INV/accept" '{"handle":"Bobur"}')
has "$body" '"handle":"bobur"' "accepted"
has "$body" '"already":false' "joined"
body=$(BP "/api/v1/web/invites/$INV/accept" '{"handle":"bobur2"}')
has "$body" "already used" "single use"
body=$(curl -s -b "$BJ" "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm")
has "$body" '"role":"member"' "bobur is a member"
has "$body" '@bobur joined the project as member.' "join notice in the room"
body=$(BP /api/v1/web/projects/dilnoza-lab/crm/invites '{"role":"viewer"}')
has "$body" '"error":"forbidden"' "members cannot invite"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/members/bobur/role '{"role":"viewer"}')
has "$body" '"handle":"bobur"' "role change returns members"
body=$(BP /api/v1/web/projects/dilnoza-lab/crm/messages '{"text":"salom"}')
has "$body" '"error":"forbidden"' "a viewer cannot post"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/members/dilnoza/role '{"role":"member"}')
has "$body" "your own role" "nobody changes their own role"
# Hand-over: a person added from the CLI claims their member through a link.
"$HUB" member add sardor --kind human --org dilnoza-lab --project crm --role viewer >/dev/null
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/invites '{"role":"maintainer","member":"@sardor"}')
INV2=$(code_of "$body")
has "$body" '"member":"sardor"' "hand-over invite"
SJ="$WORK/sardor.txt"
curl -s -c "$SJ" "${J[@]}" "${CSRF[@]}" "${IP2[@]}" -d '{"name":"Sardor","email":"sardor@qitmir.uz","password":"sardor-password"}' "$DAVRA_HUB/api/v1/auth/register" >/dev/null
body=$(curl -s -b "$SJ" "${J[@]}" "${CSRF[@]}" -X POST "$DAVRA_HUB/api/v1/web/invites/$INV2/accept")
has "$body" '"handle":"sardor"' "claimed the existing member"
body=$(curl -s -b "$SJ" "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm")
has "$body" '"role":"maintainer"' "the hand-over sets the invited role"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/invites '{"role":"member","member":"sardor"}')
has "$body" "already has a Davra account" "no hand-over of a member with an account"
# Same role as the CLI gave: still a join, the link is spent and the room hears it.
"$HUB" member add aziz --kind human --org dilnoza-lab --project crm --role maintainer >/dev/null
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/invites '{"role":"maintainer","member":"aziz"}')
INV4=$(code_of "$body")
AJ="$WORK/aziz.txt"
curl -s -c "$AJ" "${J[@]}" "${CSRF[@]}" "${IP2[@]}" -d '{"name":"Aziz","email":"aziz@qitmir.uz","password":"aziz-password"}' "$DAVRA_HUB/api/v1/auth/register" >/dev/null
body=$(curl -s -b "$AJ" "${J[@]}" "${CSRF[@]}" -X POST "$DAVRA_HUB/api/v1/web/invites/$INV4/accept")
has "$body" '"already":false' "same-role hand-over counts as a join"
has "$(curl -s "$DAVRA_HUB/api/v1/web/invites/$INV4")" '"status":"used"' "same-role hand-over spends the link"
has "$(curl -s -b "$AJ" "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm")" '@aziz joined the project as maintainer.' "same-role hand-over is announced"
# Revoke.
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/invites '{"role":"viewer"}')
INV3=$(code_of "$body")
IID=$(grep -o '"id":"[0-9a-f-]*"' <<<"$body" | head -1 | cut -d'"' -f4)
body=$(W "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/invites")
has "$body" "$IID" "open invites listed"
code=$(W -o /dev/null -w '%{http_code}' "${CSRF[@]}" -X POST "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/invites/$IID/revoke")
[ "$code" = 204 ] || fail "revoke invite -> $code"
has "$(curl -s "$DAVRA_HUB/api/v1/web/invites/$INV3")" '"status":"revoked"' "revoked invite"
# Remove and leave.
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$SJ" "${CSRF[@]}" -X POST "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/members/dilnoza/remove")
[ "$code" = 403 ] || fail "a maintainer cannot remove the owner -> $code"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$BJ" "${CSRF[@]}" -X POST "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/members/bobur/remove")
[ "$code" = 204 ] || fail "leave -> $code"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$BJ" "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm")
[ "$code" = 404 ] || fail "after leaving the project is gone -> $code"
code=$(W -o /dev/null -w '%{http_code}' "${CSRF[@]}" -X POST "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/members/dilnoza/remove")
[ "$code" = 403 ] || fail "an owner cannot leave -> $code"
ok "team: invite links (single use, public info, revoke), hand-over of CLI members, roles by rank, leave and remove"

echo "== installers"
out=$(curl -fs "$DAVRA_HUB/install.sh")
has "$out" "HUB='$DAVRA_HUB'" "install.sh points at this hub"
has "$out" "checksum mismatch" "install.sh verifies checksums"
out=$(curl -fs "$DAVRA_HUB/install.ps1")
has "$out" "\$hub = '$DAVRA_HUB'" "install.ps1 points at this hub"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/agents/dilnoza-ai/tokens '{"name":"second"}')
has "$body" "irm $DAVRA_HUB/install.ps1 | iex" "connect commands include the installer"
ok "one-line installers served by the hub; connect commands include them"

echo "== rename"
as "$T_LAI" post "@lionali tekshirib ko'ring" --project mosla >/dev/null
"$HUB" member rename lionali ali_9737 --org shaxsiy >/dev/null
out=$(as "$T_LI" whoami)
has "$out" "@ali_9737 (human)" "renamed handle"
out=$(as "$T_LI" inbox --project mosla)
has "$out" "tekshirib ko'ring" "old mentions follow the rename"
out=$(as "$T_LAI" brief --project mosla)
has "$out" "@lionali-ai (agent of @ali_9737)" "agents see the new owner handle"
out=$("$HUB" member rename gulom ali_9737 --org shaxsiy 2>&1 || true)
has "$out" "already used" "rename cannot take a used handle"
ok "rename: handle changes, past mentions follow, collisions refused"

echo "== usage (OpenTelemetry)"
body=$(WP /api/v1/web/projects/dilnoza-lab/crm/agents/dilnoza-ai/tokens '{"name":"otel"}')
OT=$(grep -o '"token":"dvr_[a-f0-9]*"' <<<"$body" | cut -d'"' -f4)
NOW="$(date +%s)000000000"
otlp() { # temporality cost tokens
  printf '{"resourceMetrics":[{"resource":{"attributes":[{"key":"user.email","value":{"stringValue":"x@y.z"}}]},"scopeMetrics":[{"metrics":[
{"name":"claude_code.cost.usage","unit":"USD","sum":{"aggregationTemporality":%s,"dataPoints":[{"attributes":[{"key":"model","value":{"stringValue":"claude-opus-5-5"}}],"timeUnixNano":"%s","asDouble":%s}]}},
{"name":"claude_code.token.usage","sum":{"aggregationTemporality":%s,"dataPoints":[{"attributes":[{"key":"model","value":{"stringValue":"claude-opus-5-5"}},{"key":"type","value":{"stringValue":"output"}}],"timeUnixNano":"%s","asInt":"%s"}]}},
{"name":"claude_code.lines_of_code.count","sum":{"aggregationTemporality":1,"dataPoints":[{"attributes":[{"key":"type","value":{"stringValue":"added"}}],"timeUnixNano":"%s","asInt":"7"}]}},
{"name":"claude_code.active_time.total","sum":{"aggregationTemporality":1,"dataPoints":[{"attributes":[{"key":"type","value":{"stringValue":"cli"}}],"timeUnixNano":"%s","asDouble":30}]}}
]}]}]}' "$1" "$NOW" "$2" "$1" "$NOW" "$3" "$NOW" "$NOW"
}
post_otlp() { curl -s -H "Authorization: Bearer $1" -H 'content-type: application/json' -H "x-davra-project: $2" "${@:3}" --data-binary @- "$DAVRA_HUB/otlp/v1/metrics"; }
out=$(otlp 1 0.25 1000 | post_otlp "$OT" crm)
[ "$out" = '{}' ] || fail "otlp export -> $out"
otlp 1 0.5 500 | post_otlp "$OT" crm >/dev/null
out=$(otlp 2 9 9 | post_otlp "$OT" crm)
has "$out" '"rejectedDataPoints":"2"' "cumulative points are refused, not added"
code=$(otlp 1 1 1 | curl -s -o /dev/null -w '%{http_code}' -H 'content-type: application/json' --data-binary @- "$DAVRA_HUB/otlp/v1/metrics")
[ "$code" = 401 ] || fail "otlp without a token -> $code"
code=$(otlp 1 1 1 | post_otlp "$T_LAI" crm -o /dev/null -w '%{http_code}')
[ "$code" = 404 ] || fail "otlp for an ungranted project -> $code"
body=$(W "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/usage?days=7")
has "$body" '"cost_usd":0.75' "usage sums deltas"
has "$body" '"output_tokens":1500' "token usage"
has "$body" '"lines_added":21' "lines"
has "$body" '"active_secs":90.0' "active time"
has "$body" '"handle":"dilnoza-ai","owner":"dilnoza"' "per member"
has "$body" '"model":"claude-opus-5-5"' "per model"
hasnt "$body" 'x@y.z' "identity attributes are dropped"
has "$(W "$DAVRA_HUB/api/v1/web/projects/dilnoza-lab/crm/usage")" '"days":14' "default window"
has "$body" "\"first_day\":\"$(date +%Y-%m-%d)\"" "first day reported"
out=$(DAVRA_TOKEN="$OT" "$CLI" otel-headers --hub "$DAVRA_HUB" --project crm)
has "$out" "\"Authorization\":\"Bearer $OT\"" "otel-headers prints the token"
has "$out" '"X-Davra-Project":"crm"' "otel-headers names the project"
out=$(DAVRA_HOME="$(mktemp -d)" "$CLI" otel-headers --hub "$DAVRA_HUB" --project crm)
[ "$out" = '{}' ] || fail "otel-headers when logged out -> $out"
ok "usage: Claude Code OTLP metrics summed per member/day/model, cumulative refused, token required, web report"

echo "== row-level security"
grep -q "row-level security on" "$WORK/hub.log" || fail "the hub should report row-level security on"
out=$("$HUB" rls-check 2>&1) || fail "rls-check: $out"
has "$out" "row-level security ok" "every org sees only its own rows"
ok "row-level security: tables with org_id protected, unknown org sees nothing, each org sees only its rows"

echo
echo "e2e: all $pass checks passed"
