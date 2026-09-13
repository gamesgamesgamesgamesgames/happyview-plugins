# happyview-db

Library plugin exposing `happyview.db`: a chainable builder over indexed
records. It folds the chain into a query spec and sends it to one host
import; the host generates the SQL and owns the schema.

## Lua

```lua
local db = require("happyview.db")

local page = db.records("app.bsky.feed.post")
  :where("author", "=", caller_did)
  :where("text", "like", "%happyview%")
  :sort("createdAt", "desc")
  :limit(20)
  :run()

local total = db.records("app.bsky.feed.post"):where("author", "=", caller_did):count()
local newest = db.records("app.bsky.feed.post"):sort("createdAt", "desc"):first()

local one = db.get("at://did:plc:abc/app.bsky.feed.post/xyz")
local hits = db.search("app.bsky.feed.post", "text", "happyview", 10)
local backend = db.backend() -- "sqlite" or "postgres"
```

## Surface

- `records(collection)` — constructor. Lazy `where(field, op, value)`,
  `sort(field, direction)`, `limit(n)`, `cursor(c)`, `did(d)`. Immediate
  `run()` → `{records, cursor}`, `count()` → integer, `first()` → record or
  nil.
- `get(uri)` — one record by AT URI, or nil.
- `search(collection, field, query, limit?)` — substring search on one
  field, ranked by match position. Default limit 10, max 100.
- `backend()` — `"sqlite"` or `"postgres"`, from the call context the host
  fills on every call.

`where` fields are dotted JSON paths with optional array indices, validated
by the host. Operators: `=`, `!=`, `<`, `>`, `<=`, `>=`, `like`,
`not like`, `ilike` (case-insensitive on input; the host lowers it to
`LIKE`/`ILIKE` per backend). Filters send the value as given; the host
compares record fields as text regardless of its JSON type. Repeated `where`
steps combine with `AND`. `limit` defaults to 20 and caps at 100.

Pagination depends on `sort`: the default sort paginates by a `(created_at,
uri)` keyset cursor; a custom `sort` paginates by offset cursor instead.
Both move opaquely through `cursor()`.

## Errors

- `BAD_CHAIN` — the object document itself is malformed (unknown step,
  unknown operator, bad sort direction, non-numeric limit). Raised by this
  crate before any host call.
- `INVALID_SPEC` — the host rejected an otherwise well-formed spec (bad
  field path). Passed through unchanged.

## Capabilities

`records:read`. Imports exactly `host_records_query`, `host_records_count`,
`host_records_get`, `host_records_search`.

## Build

```bash
cargo test -p happyview-db
cargo build --release --target wasm32-unknown-unknown -p happyview-db
```
