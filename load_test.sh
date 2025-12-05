#!/bin/bash
echo "Running 20 ingest requests..."
TIMES=""
for i in $(seq 1 20); do
  NOW=$(date +%s)000
  TIME=$(curl -s -w "%{time_total}" -o /dev/null -X POST https://overwatch-ingestion.fly.dev/overwatch-ingest \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer owk_live_fH1uColFx9U8LjZ42mQNjG93QoCuxXGP" \
    -d "[{\"id\": \"load-$i-$RANDOM\", \"type\": \"pageview\", \"timestamp\": $NOW, \"sessionId\": \"load-test-$i\", \"url\": \"https://test.com/page-$i\", \"userAgent\": \"curl/load-test\"}, {\"id\": \"click-$i-$RANDOM\", \"type\": \"click\", \"timestamp\": $NOW, \"sessionId\": \"load-test-$i\", \"url\": \"https://test.com/page-$i\", \"userAgent\": \"curl/load-test\", \"data\": {\"x\": $i, \"y\": $i}}]")
  echo "Request $i: ${TIME}s"
  TIMES="$TIMES $TIME"
done

echo ""
echo "=== Summary ==="
echo "$TIMES" | tr ' ' '\n' | grep -v "^$" | awk '{sum+=$1; count++; if($1>max)max=$1; if(min==""||$1<min)min=$1} END {printf "Total requests: %d\nMin latency: %.3fs\nMax latency: %.3fs\nAvg latency: %.3fs\n", count, min, max, sum/count}'
