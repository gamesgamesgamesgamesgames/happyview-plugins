# happyview-backlinks

Library plugin exposing `happyview.backlinks`: a chainable query for records
that reference a given AT URI. A backlink is a record whose strong refs
point at the target URI, indexed in `happyview_record_refs`. The chain
folds into a query spec and sends it to one host import; the host owns the
join.

## Lua

```lua
local backlinks = require("happyview.backlinks")

local page = backlinks
  .to("at://did:plc:a/app.bsky.feed.post/1")
  :collection("app.bsky.feed.like")
  :did("did:plc:b")
  :limit(20)
  :run()

for _, record in ipairs(page.records) do
  -- record is the liking record, not the post it points at
end

local next_page = backlinks
  .to("at://did:plc:a/app.bsky.feed.post/1")
  :collection("app.bsky.feed.like")
  :cursor(page.cursor)
  :run()
```

## Surface

- `to(uri)` — constructor. Lazy `collection(nsid)`, `did(did)`, `limit(n)`,
  `cursor(cursor)`. Immediate `run()` → `{records, cursor}`.
- `collection` is required: the host's join needs a collection to scope the
  search, and omitting it is rejected before any host call.
- `did` narrows to backlinks authored by one repo. `limit` is capped by the
  host. `cursor` continues a prior page; a page with no `cursor` in its
  result is the last one.

## Errors

- `BAD_INPUT` — `to()` was called with no URI.
- `BAD_CHAIN` — the chain document itself is malformed: a missing
  `collection`, an unknown step, or a non-numeric `limit`. Raised by this
  crate before any host call.
- `FORBIDDEN` / `DB_ERROR` — the host rejected an otherwise well-formed
  call. Passed through unchanged.

## Capabilities

`records:read`. Imports exactly `host_backlinks_query`.

## Build

```bash
cargo test -p happyview-backlinks
cargo build --release --target wasm32-unknown-unknown -p happyview-backlinks
```
