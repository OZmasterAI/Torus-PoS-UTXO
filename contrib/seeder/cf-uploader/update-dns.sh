#!/bin/bash
# Reads dnsseed.dump and pushes good node IPs as A records to Cloudflare DNS.
# Usage: ./update-dns.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DUMP_FILE="$SCRIPT_DIR/../dnsseed.dump"
CONF_FILE="$SCRIPT_DIR/../settings.conf"

CF_API_TOKEN=$(grep cf_api_token "$CONF_FILE" | cut -d= -f2 | tr -d '"' | tr -d ' ')
CF_ZONE_ID=$(grep cf_zone_id "$CONF_FILE" | cut -d= -f2 | tr -d '"' | tr -d ' ')
CF_DOMAIN_PREFIX=$(grep cf_domain_prefix "$CONF_FILE" | cut -d= -f2 | tr -d '"' | tr -d ' ')
CF_DOMAIN=$(grep cf_domain "$CONF_FILE" | cut -d= -f2 | tr -d '"' | tr -d ' ')
WALLET_PORT=$(grep wallet_port "$CONF_FILE" | cut -d= -f2 | tr -d '"' | tr -d ' ')
MAX_SEEDS=25

RECORD_NAME="${CF_DOMAIN_PREFIX}.${CF_DOMAIN}"
CF_API="https://api.cloudflare.com/client/v4"

if [ -z "$CF_API_TOKEN" ] || [ -z "$CF_ZONE_ID" ]; then
    echo "ERROR: cf_api_token and cf_zone_id must be set in settings.conf"
    exit 1
fi

if [ ! -f "$DUMP_FILE" ]; then
    echo "ERROR: $DUMP_FILE not found. Is the seeder running?"
    exit 1
fi

# Parse good IPs from dump file (port matches and status is 1)
GOOD_IPS=()
while IFS= read -r line; do
    [[ "$line" =~ ^# ]] && continue
    [[ -z "$line" ]] && continue
    IP_PORT=$(echo "$line" | awk '{print $1}')
    STATUS=$(echo "$line" | awk '{print $2}')
    IP=$(echo "$IP_PORT" | cut -d: -f1)
    PORT=$(echo "$IP_PORT" | cut -d: -f2)
    if [ "$PORT" = "$WALLET_PORT" ] && [ "$STATUS" = "1" ]; then
        GOOD_IPS+=("$IP")
    fi
done < "$DUMP_FILE"

echo "Found ${#GOOD_IPS[@]} good nodes in dump file"

if [ ${#GOOD_IPS[@]} -eq 0 ]; then
    echo "No good seeds found yet. Seeder needs more time to crawl."
    exit 0
fi

# Get current DNS records for this prefix
CURRENT=$(curl -s -X GET "$CF_API/zones/$CF_ZONE_ID/dns_records?type=A&name=$RECORD_NAME&per_page=100" \
    -H "Authorization: Bearer $CF_API_TOKEN" \
    -H "Content-Type: application/json")

CURRENT_IDS=($(echo "$CURRENT" | python3 -c "import sys,json; [print(r['id']) for r in json.load(sys.stdin).get('result',[])]" 2>/dev/null))
CURRENT_IPS=($(echo "$CURRENT" | python3 -c "import sys,json; [print(r['content']) for r in json.load(sys.stdin).get('result',[])]" 2>/dev/null))

echo "Current Cloudflare records: ${#CURRENT_IPS[@]}"

# Delete stale records (IPs no longer in good list)
for i in "${!CURRENT_IPS[@]}"; do
    IP="${CURRENT_IPS[$i]}"
    ID="${CURRENT_IDS[$i]}"
    FOUND=0
    for GOOD in "${GOOD_IPS[@]}"; do
        [ "$GOOD" = "$IP" ] && FOUND=1 && break
    done
    if [ $FOUND -eq 0 ]; then
        echo "Removing stale: $IP"
        curl -s -X DELETE "$CF_API/zones/$CF_ZONE_ID/dns_records/$ID" \
            -H "Authorization: Bearer $CF_API_TOKEN" > /dev/null
    fi
done

# Add new records (up to MAX_SEEDS)
COUNT=${#CURRENT_IPS[@]}
for GOOD in "${GOOD_IPS[@]}"; do
    [ $COUNT -ge $MAX_SEEDS ] && break
    EXISTING=0
    for CUR in "${CURRENT_IPS[@]}"; do
        [ "$CUR" = "$GOOD" ] && EXISTING=1 && break
    done
    if [ $EXISTING -eq 0 ]; then
        echo "Adding: $GOOD"
        curl -s -X POST "$CF_API/zones/$CF_ZONE_ID/dns_records" \
            -H "Authorization: Bearer $CF_API_TOKEN" \
            -H "Content-Type: application/json" \
            --data "{\"type\":\"A\",\"name\":\"$RECORD_NAME\",\"content\":\"$GOOD\",\"ttl\":120,\"proxied\":false}" > /dev/null
        COUNT=$((COUNT + 1))
    fi
done

echo "Done. Total records: $COUNT"
