#!/usr/bin/env bash
# Semantic search check against a real embedder (Ollama + bge-m3).
#   DATABASE_URL=postgres://... DAVRA_EMBED_URL=http://ollama:11434 BIN=target/debug scripts/e2e-semantic.sh
set -euo pipefail
BIN=${BIN:-target/debug}
HUB="$BIN/davra-hub"; CLI="$BIN/davra"
PORT=${PORT:-18081}
export DAVRA_HUB="http://127.0.0.1:$PORT" DAVRA_HOME="$(mktemp -d)"
fail() { echo "  FAIL $*" >&2; exit 1; }

"$HUB" migrate >/dev/null
"$HUB" org create sem >/dev/null
"$HUB" project create mosla >/dev/null
"$HUB" member add ali --kind human --project mosla >/dev/null
T=$("$HUB" token create ali 2>/dev/null)
"$HUB" serve --bind "127.0.0.1:$PORT" --no-migrate --cookie-secure false --embed-url "$DAVRA_EMBED_URL" >/tmp/sem-hub.log 2>&1 &
PID=$!; trap 'kill $PID 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do curl -fs "$DAVRA_HUB/healthz" >/dev/null 2>&1 && break; sleep 0.1; done
curl -fs "$DAVRA_HUB/.well-known/davra.json" | grep -q 'search:semantic' || fail "semantic feature not advertised"

post() { DAVRA_TOKEN=$T "$CLI" post "$1" --project mosla >/dev/null; }
post "Textarea komponentida maxLength endi qattiq cheklov emas, faqat ogohlantirish beradi"
post "Deploy pipeline uchun Cloudflare tunnel tokenini yangiladim"
post "Nur Dental brendi uchun yangi binafsha rang palitrasi qo'shildi"
post "Kontaktlar ro'yxati endi cursor bilan sahifalanadi"

for _ in $(seq 1 60); do
  n=$(grep -c "embedding batch failed" /tmp/sem-hub.log || true)
  [ "$n" -gt 0 ] && { tail -3 /tmp/sem-hub.log; fail "embedder errors"; }
  left=$(psql "$DATABASE_URL" -Atc "select count(*) from messages where embedding is null" 2>/dev/null || echo 1)
  [ "$left" = 0 ] && break; sleep 1
done
[ "$left" = 0 ] || fail "messages not embedded in time"

# No shared words with the target message: only meaning connects them.
out=$(DAVRA_TOKEN=$T "$CLI" search "matn maydonining uzunlik limiti" --project mosla -k 1)
echo "$out" | head -2
grep -q "Textarea" <<<"$out" || fail "semantic match (uz paraphrase)"
out=$(DAVRA_TOKEN=$T "$CLI" search "pagination of the contacts list" --project mosla -k 1)
grep -q "Kontaktlar" <<<"$out" || fail "semantic match (en -> uz)"
echo "semantic: ok (uz paraphrase and en->uz cross-lingual)"
