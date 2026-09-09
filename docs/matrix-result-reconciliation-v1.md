# Matrix Durable Result Lookup and Reconciliation v1

Status: repository source implemented; exact-SHA runtime qualification pending  
Candidate authority: Sequence 54 integration  
Production authorization: **not granted**

## 1. Purpose

This contract closes the repository-side gap where the Matrix relay can lose an
adapter response after Consumer Entry has already accepted and durably recorded
the exact Matrix operation result. A timeout, network failure, interrupted body,
invalid response, unknown status, duplicate marker or internal post-I/O failure
must never be interpreted as proof of success and must never trigger an unbound
blind replay.

Recovery is deliberately split into two authorities:

1. a read-only, principal-bound lookup of the exact Consumer Entry replay record;
2. a least-privilege PostgreSQL transition that stores immutable lookup evidence,
   persists the exact result payload and closes only the matching held adapter
   delivery.

Neither phase calls the Gateway, creates an invocation, repeats the business
request, advances a Matrix cursor or sends a Matrix event. Enqueuing a user-visible
reply remains a separate reviewed action.

## 2. Components

### 2.1 Consumer Entry lookup

Production binary wiring:

- source: `services/consumer-entry-api/src/matrix_result_lookup.rs`;
- binary integration: `services/consumer-entry-api/src/main.rs`;
- endpoint: `POST /v1/matrix/messages/result`;
- durable source: `CONSUMER_ENTRY_REPLAY_STORE_PATH`;
- exact cache key: `matrix-event:<event_id>`.

Request body:

```json
{
  "event_id": "$matrix-event-id",
  "matrix_user_id": "@principal:homeserver",
  "room_id": "!room:homeserver"
}
```

The request rejects unknown JSON fields and enforces bounded Matrix identifiers.
It requires both the Consumer Entry ingress token and a signed session assertion
bound to the exact event, Matrix principal, room, issuer, key, audience, issue
and expiry times, and a request fingerprint.

Consumer Entry returns a result only when the stable regular-file snapshot parses,
the exact entry exists, remains in the replay window, contains a recorded response,
and the stored source, identity scope and task/invocation identity all match.
Missing evidence is not proof that no effect occurred.

### 2.2 Adapter read-only lookup

Production facade wiring:

- source: `services/matrix-entry-adapter/src/result_reconciliation.rs`;
- facade integration: `services/matrix-entry-adapter/src/lib.rs`;
- endpoint: `POST /v1/matrix/results/lookup`.

The adapter endpoint authenticates its own `x-entry-token`, creates the exact
Consumer Entry session assertion from its validated issuer/key configuration,
and calls only the Consumer Entry lookup endpoint. It does not call
`/v1/matrix/messages`, `/v1/invocations` or any mutation route.

A successful adapter response contains:

- `accepted = true`;
- `action = task_result_reconciled`;
- the exact event, room and sender;
- the original cached result in `forwarded`;
- `projected_reply = null`;
- `reconciliation.schema = cex.matrix.adapter-result-reconciliation.v1`;
- `reconciliation.source = consumer_entry_durable_replay`;
- `reconciliation.read_only = true`.

### 2.3 Runtime reconciliation command

`scripts/reconcile-matrix-adapter-result.py` performs one bounded reconciliation.
It requires an operator to supply the immutable delivery, event, payload hash,
room, sender and candidate identities. It then:

1. validates canonical UUID, Matrix identifiers, SHA-256 values and exact 40-hex
   candidate commit;
2. constructs the adapter lookup URL without userinfo, query, fragment or redirect;
3. requires HTTPS, except for an explicit loopback-only local test switch;
4. reads `MATRIX_ENTRY_INGRESS_TOKEN` from the environment;
5. executes one bounded adapter lookup and rejects duplicate JSON keys;
6. independently validates all outer, source, principal-scope and task identities;
7. hashes the exact lookup response bytes;
8. sends the result and evidence to PostgreSQL only through standard input;
9. invokes only `cex_matrix_reconcile_adapter_result_v1` using the configured
   reconciliation database identity;
10. emits a bounded result containing identities, hashes and disposition, never
    tokens, database credentials or raw result bytes.

Database credentials are read from `MATRIX_RECONCILIATION_DATABASE_URL`, converted
to libpq environment variables and removed from child process arguments. The
runtime identity must inherit only `cex_matrix_reconciler_runtime`; direct table
DML and delivery claiming remain denied.

Example:

```text
MATRIX_ENTRY_INGRESS_TOKEN=... \
MATRIX_RECONCILIATION_DATABASE_URL=postgresql://reconciler:...@db/cex \
python3 scripts/reconcile-matrix-adapter-result.py \
  --adapter-base-url https://matrix-adapter.internal \
  --delivery-id 61000000-0000-4000-8000-000000000001 \
  --event-id '$event' \
  --room-id '!room:example' \
  --sender '@alice:example' \
  --payload-sha256 sha256:... \
  --candidate-sha 0123456789abcdef0123456789abcdef01234567
```

Shell history and procepÈ[œÜXİ[ÛˆØ[ˆİ[^ÜÙH›Û‹\ÙXÜ™]Y[YšY\œË‚“Ü\˜]ÜœÈ]\İ\ÙH[ˆ\›İ™Y^Xİ][Ûˆİ\™˜XÙH[™]šY[˜ÙK\™][[ÛˆÛXŞK‚‚ˆÈÈËˆÜİÜ™TÔS]]Üš]H[™[[]]X›H]šY[˜ÙB‚“Ü\˜]ÜˆZYÜ˜][ÛœÈ\™H\YY[ˆÜ™\‚‚ŒKˆWØY\\—Ü™\İ[Ü™XÛÛ˜Ú[X][Û‹œÜ[ÂŒ‹ˆ—Ü[[YWÜ›Û\ËœÜ[ÂŒËˆ×ØY\\—Ü™\İ[Ù]šY[˜ÙWØš[™[™ËœÜ[ÂˆØY\\—Ü™\İ[Ü[[YWÜ™XÛÛ˜Ú[X][Û‹œÜ[‚‚“ZYÜ˜][Ûˆš^\ÈHÛÛ˜Ü™]H[[YHY™Xİ[ˆHš[Üˆ™\XÙ[Y[[˜İ[Û‚ŒÈ˜[Y]YÜ™\İ[Ü^[ØY]ÛZ]Y]œ›ÛHH[œÙ\[ÈH“Õ•S˜™\İ[Ü^[ØYÛÛ[[‹ˆš\œİİXØÙ\ÜÙ[™XÛÛ˜Ú[X][Ûˆ\™Y›Ü™HÛİ[›İ˜ÛÛ[Z]ˆH™\XÙ[Y[[˜İ[Ûˆ›İÎ‚‚‹H\œÚ\İÈH^Xİ˜[Y]Y™\İ[^[ØYÂ‹HÛÛ\\™\È^[ØY\Ú]šY[˜ÙH[™Y[]HÛˆ™\^NÂ‹HÜš]\È^XİHÛ™HXYÛ]\ˆOˆÙ[\İÜH™XÛÜ™Â‹H™Z™XİÈH™\^H[›\ÜÈH[]™\H™[XZ[œÈ\›Z[˜[Ù[Â‹HYZ]ÈÛ›H™[^Hİ]ÛÛY\È^XÚ]HÛ\ÜÚYšYY\È[šÛ›İÛÂ‹H™\Ù\™\ÈHØ[YHŒH[˜İ[ÛˆÚYÛ˜]\™H[™X\İ\š]š[YÙHÜ˜[Â‹H™Z™XİÈX›XÈ^Xİ][Ûˆ[™\™Xİ]šY[˜ÙK]X›H]]][Û‹‚‚”™XÛÛ˜Ú[X›H˜Z[\™HÛÙ\È\™HHÛÜÙY[šÛ›İÛ‹[İ]ÛÛYH˜[Z[B˜Y\\—Ü™\ÜÛœÙWİ[šÛ›İÛ—Ê˜\Î‚‚‹HY\\—İ[™\šYšYYÛİ™\œÚ^™YÜ™\ÜÛœÙXÂ‹HY\\—Ù\XØ]WÛİ]ÛÛYWİ[šÛ›İÛ˜Â‹H™[^WÚ[\›˜[İ[šÛ›İÛ—Ûİ]ÛÛYX‚‚HYš[š]HY\\ˆ™Z™Xİ[ÛˆİXÚ\ÈY\\—Ü\›X[™[Üİ]\ØØ[››İ™HÛÛ™\YÈİXØÙ\ÜÈH\È]‚‚•H]šY[˜ÙHØš™Xİ\ÈHÛÜÙYØš™XİÛÛZ[š[™Î‚‚˜œÛÛ‚ÂˆœØÚ[XHˆ˜Ù^›X]š^˜Y\\‹\™\İ[\™XÛÛ˜Ú[X][Û‹Y]šY[˜ÙKŒH‹ˆ›ÛÚİ\Ü™\ÜÛœÙWÜÚLMˆˆœÚLM‹‹‹ˆ‹ˆ›ØœÙ\™YØ]ˆŒŒ‹LKLUŒŒˆ‹ˆ˜Ø[™Y]WÜÚHˆŒLŒÍMÎXX˜ÙYŒLŒÍMÎXX˜ÙYŒLŒÍMÈ‚ŸB˜‚•H]X˜\ÙH™XÛÛ\]\ÈHÒKLMˆÙˆÜİÜ™TÔS	ÜÈØ[›ÛšXØ[œÛÛ˜^œ™\™\Ù[][Û‹ˆÛY[\ÚYH›Ü›X][™ÈØ[››İÙ[XİHY™™\™[İÜ™Y\Ú‚‚ˆÈÈˆ˜Z[\™HÙ[X[XÜÂ‚ŸÛÛ™][ÛˆÛÛœİ[Y\ˆ[HY\\ˆ[[YHÛÛ[X[™È]X˜\ÙHŸKK_KK_KK_KK_Ÿ[šÛ›İÛˆ™\İ[[›Èİ]HÚ[™ÙHŸ^\™Y™\^H]šY[˜ÙHL[›Èİ]HÚ[™ÙHŸX\šÙ\ˆÚ]İ]™\ÜÛœÙHX[›Èİ]HÚ[™ÙHŸY[]HÜˆØÛÜHÛÛ™›XİX[™Z™XİYŸ]]˜Z[\™HX[™Z™XİYŸ™\^HİÜ™H[˜]˜Z[X›HLØ[™Z™XİYŸ™Y\™Xİ›Û‹RÈ™[[İHT“‹ØH‹ØH™Z™XİY™Y›Ü™H™\]Y\İŸİ™\œÚ^™YÚ[\œ\YÛ›Û‹R”ÓÓˆ™\ÜÛœÙH‹ØH[™Z™XİYŸ[œİ\ÜYXY[]\ˆ™X\ÛÛˆ‹ØH‹ØH™Z™XİYŸY[XØ[ÛÛ\]Y™XÛÛ˜Ú[X][Ûˆ‹ØH‹ØH™\^X›È™]È\İÜHŸØ[YHY[]HÚ]Ú[™ÙYÛÛ[‹ØH‹ØHÛÛ\Ú[Û‹›È]]][Ûˆ‚“›È˜Z[\™H]™]\›œÈİXØÙ\ÜË™\X]ÈH\Ú[™\ÜÈ™\]Y\İ^ÜÙ\ÈÙXÜ™]ÈÜ‚˜Ú[™Ù\È[]™\Hİ]HÚ]İ]H[[]]X›H]šY[˜ÙH[œÙ\[ˆHØ[YH]X˜\ÙB˜[œØXİ[Û‹‚‚ˆÈÈKˆ™\šYšXØ][Û‚‚”™\ÜÚ]ÜHÛİ\˜ÙHØ]N‚‚˜^œ]ÛŒÈØÜš\ËØÚXÚË[X]š^\™\İ[\™XÛÛ˜Ú[X][Û‹œBœ]ÛŒÈØÜš\ËÜ™XÛÛ˜Ú[K[X]š^XY\\‹\™\İ[œHK\Ù[‹]\İ˜‚”\İØ]\Î‚‚˜^˜Ø\™ÛÈ›]\X]š^Y[KXY\\ˆ\ÛÛœİ[Y\‹Y[KX\HKHKXÚXÚÂ˜Ø\™ÛÈ\İK[ØÚÙY\X]š^Y[KXY\\ˆKX[]\™Ù]Â˜Ø\™ÛÈ\İK[ØÚÙY\ÛÛœİ[Y\‹Y[KX\HKX[]\™Ù]Â˜Ø\™ÛÈÛ\HK[ØÚÙY\X]š^Y[KXY\\ˆKX[]\™Ù]ÈKHQØ\›š[™ÜÂ˜Ø\™ÛÈÛ\HK[ØÚÙY\ÛÛœİ[Y\‹Y[KX\HKX[]\™Ù]ÈKHQØ\›š[™ÜÂ˜‚”ÜİÜ™TÔSØ]N‚‚˜^˜˜\ÚØÜš\ËØÚXÚË[X]š^[Ü\˜]Ü‹\ÜİÜ™\ËœÚ˜‚•HÜİÜ™TÔS[›™\ˆ\Y\ÈH›İ\ˆÜ\˜]ÜˆZYÜ˜][ÛœÈÚXÙH[™^Xİ]\ÈB˜˜\Ù[[™K]šY[˜ÙKZ\™[š[™È[™[[YK\™XÛÛ˜Ú[X][Ûˆ™YÜ™\ÜÚ[ÛœÈ[ˆÛ™H›İ[™Y”ÜİÜ™TÔSMˆÙ\ÜÚ[Û‹ˆH[[YH™YÜ™\ÜÚ[Ûˆ›İ™\Èš\œİÛÛ\][Û‹^Xİœ^[ØY\œÚ\İ[˜ÙKÚYKYY™™XİYœ™YH™\^KÛ™H\İÜH˜[œÚ][Ûˆ[™Ú[™ÙYœ^[ØYÛÛ\Ú[Ûˆ™Z™Xİ[Û‹‚‚HÛİ\˜ÙHÚXÚÙ\‹Ù[‹]\İ]Y]YY›Ø‹™\›Ë\İ\›ØˆÜˆÙ[™\˜]Y”ÓÓˆ\È›İ”ÜİÜ™TÔSÜˆÜİY]X[YšXØ][Ûˆ]šY[˜ÙK‚‚ˆÈÈ‹ˆ™\]Z\™Y]šY[˜ÙH™Y›Ü™H›Û[İ[Û‚‚•H™\ÜÚ]ÜK\ÚYH[\[Y[][Ûˆ\ÈÛÛ\]HÛ›HÚ[ˆÛ™H[˜Ú[™ÙYØ[™Y]B”ÒKİ™YH\È[ÙˆH›ÛİÚ[™È›Û‹Y[\H]šY[˜ÙN‚‚ŒKˆÛÛœİ[Y\ˆ[KY\\ˆ[™™XÛÛ˜Ú[X][ÛˆÛİ\˜ÙHØ]\È\ÜË‚Œ‹ˆHÛÛ\]HX]š^Ü\˜]ÜˆZYÜ˜][ÛˆÚZ[ˆ[œÈÛˆ\ÜÜØX›HÜİÜ™TÔSM‹‚ŒËˆH[[YHÛÛ[X[™Ù[‹]\İ[™Üİ[HÔS™YÜ™\ÜÚ[ÛœÈ\ÜË‚ˆHÛÛ›ÛY™\ÜÛœÙK[ÜÜÈ™ZX\œØ[™XÛİ™\œÈH^Xİ[[]™\K‚KˆÜ›Û™È\Ù\‹›ÛÛK]™[^[ØY\ÚËÙ^K\ÜİY\‹]YY[˜ÙKš[™Ù\œš[ˆİ[H\ÜÙ\[Ûˆ[™Ú[™ÙY™\^H[˜Z[Ú]İ]]]][Û‹‚‹ˆH[[YHÙÚ[ˆ\ÈÛ›HH™XÛÛ˜Ú[\ˆ›ÛH[™Ø[››İ™XY˜]ÈX›\ËˆÛZ[H[]™\šY\Ë\ØX›HšYÙÙ\œÈÜˆ\™›Ü›H\™XİS‚ËˆH™X[ÛY\Ù\™\ˆ^\˜Ú\ÙH›İ™\È›È\XØ]H\Ú[™\ÜÈ™\]Y\İÜˆÜ›ÜÜË\›ÛÛBˆ™\HØØİ\œË‚ˆ™\]Z\™YÜİYÛÜšÙ›İÜË›İXİY[XZ[ˆÛİ™\›˜[˜ÙH[™œ™\Ú[™\[™[ˆ\›İ˜[Èš[™HØ[YHš[˜[Ø[™Y]K‚‚•\ÈÛİ\˜ÙHÚZ[ˆÛÜÙ\ÈH™]š[İ\ÛHÜ[ˆ™\ÜÚ]ÜH[\[Y[][ÛˆØ\ˆ]™Ù\È›İÜ™X]HÜİY^Xİ][Û‹^\›˜[YÙ[]˜Z[Xš[]K›ÙXİ[ÛˆÙXÜ™]˜İ\İÙK™\™\Ù[]]™H™XÛİ™\Kİ\İZ[™YÓÈ]šY[˜ÙHÜˆš[˜[[X[ˆ\›İ˜[‚•[[ÜÙH]]Üš]Y\È^\İ‚‚˜^˜[Ü[—ÙØ\×ØÛÜÙYY˜[ÙBœ›ÙXİ[Û—Ø]]Üš^˜][Û[›İÙÜ˜[Y˜