# PNS Storage Tables and RPC Methods

A plain-language guide to what data lives where and how to get to it.

---

## The Tables

Think of the whole system as a set of filing cabinets. Each cabinet (table) holds one specific kind of information. Here's what's in each one.

---

### RegistrarInfos
**Where:** `pns-registrar`
**Key:** name hash → registration details

The main registration book. Every name that has ever been registered gets a row here. Each row stores:

- **expire** — the timestamp when the name runs out and goes back up for grabs
- **capacity** — how many subnames the owner is allowed to create under this name
- **register_fee** — how much was paid to register it (this was burned, not stored anywhere, but the amount is recorded here for reference)
- **label_len** — how many characters the name is (used to calculate renewal prices)
- **last_block** — the block number when the name was last registered or renewed

If a name is not in this table, it has never been registered.

---

### NFT Tokens (Tokens)
**Where:** `pns-nft` (inside pns-registrar)
**Key:** (collection id, name hash) → token record

The ownership deeds. Every registered name is an NFT. This table stores who owns each name right now. When a name is transferred or sold, this row is updated with the new owner's account.

Each row just stores:
- **owner** — the account that currently holds this name
- **children** — how many active subnames have been created under it so far

The name hash is the token ID. There is only one collection (ID `0`) for all PNS names.

---

### OwnerToPrimaryName
**Where:** `pns-registrar`
**Key:** account → name hash

A reverse lookup table. Instead of "what name does this hash point to?", this answers "what is this account's main name?"

Each account can only have one canonical (primary) name at a time. If an account has no name registered, it has no row here. When an account registers a name, a row is added. When they release it, the row is removed.

---

### ReservedList
**Where:** `pns-registrar`
**Key:** name hash → (nothing, just presence)

A blocklist. Names in this table cannot be registered by anyone — they are reserved. The presence of a hash in this table is all that matters; there is no value stored alongside it.

---

### OfferedNames
**Where:** `pns-registrar`
**Key:** name hash → offered name record

The gift waiting room. When someone buys a name through the marketplace as a gift for someone else, the name lands here instead of going straight to the recipient. It stays here until the recipient calls `accept_offered_name`.

While a name is in this table, it is invisible to DNS lookups — `pns_getInfo` and `pns_resolveName` return null for it.

Each row stores:
- **buyer** — who bought the name
- **recipient** — who it's meant for

---

### Listings
**Where:** `pns-marketplace`
**Key:** name hash → listing record

The name shop. Names that are currently for sale on the marketplace have a row here. Each row stores:

- **seller** — the account listing the name for sale
- **price** — the asking price in the native token
- **expires_at** — the timestamp after which the listing is no longer valid

If a name has no row here, it is not for sale. A name's `for_sale` flag in `pns_getInfo` is just a quick check of whether a row exists here.

---

### Records
**Where:** `pns-resolvers`
**Key:** (name hash, record type) → bytes

The main DNS address book. For each name, this stores all of its DNS records. Each entry is a specific type of data attached to a name. The record type is a number — standard DNS types like A (1), AAAA (28), MX (15), TXT (16), and PNS custom types like SS58 address (65280), RPC endpoint (65281), validator stash (65282), and more.

For example, a name might have:
- an A record pointing to an IP address
- an SS58 record pointing to a Polkadot wallet address
- a PUBKEY record holding a public key for encrypted messaging

The SS58 record (65280) is protected — only the name owner can set it, and it cannot be overwritten by anyone else.

---

### Texts
**Where:** `pns-resolvers`
**Key:** (name hash, text key) → bytes

The notes section, separate from DNS records. Stores freeform text fields attached to a name, like a website URL, a Twitter handle, an email address, or any other human-readable metadata. Each entry has a named key (e.g. `"url"`, `"email"`, `"description"`) and a text value.

---

### RuntimeOrigin
**Where:** `pns-registrar` (registry)
**Key:** name hash → parent info

Tracks where each name sits in the naming hierarchy. For a top-level name like `alice`, it points to the root. For a subdomain like `sub.alice`, it points to `alice`. Used internally to check parent expiry when setting records on subdomains.

---

### SubnameRecords
**Where:** `pns-registrar` (registry)
**Key:** subname hash → subdomain record

The subdomain registry. Every subdomain delegation gets a row here. Each row stores:

- **parent** — the hash of the parent name (e.g. `alice` for `sub.alice`)
- **label** — the subdomain label bytes (e.g. `b"sub"`)
- **target** — the account this subdomain belongs to
- **state** — one of three states:
  - `Offered` — the parent owner offered it but the target hasn't accepted yet
  - `Active` — the target accepted; the subdomain is live
  - `Rejected` — the target explicitly rejected it (visible until the parent revokes it)

Subdomains do not have their own expiry. They expire when their parent name expires.

---

### AccountToSubnames
**Where:** `pns-registrar` (registry)
**Key:** (account, subname hash) → (nothing, just presence)

A lookup table that lists all active subnames held by a given account. Used to answer "what subnames does this account own?" without scanning all of SubnameRecords. Presence in this table means the subdomain is Active and held by this account.

---

### OfferedToAccount
**Where:** `pns-registrar` (registry)
**Key:** (account, subname hash) → (nothing, just presence)

Like AccountToSubnames, but for subdomain offers that are still pending. Lists all subnames in `Offered` state directed at a given account. Used to answer "what subdomain offers are waiting for this account to accept or reject?"

---

### Price Oracle Tables (BasePrice, RentPrice, ExchangeRate)
**Where:** `pns-price-oracle`
**Key:** index (by label length) → price tiers

Three configuration tables that control how much names cost. `BasePrice` and `RentPrice` each hold 11 price tiers — one for each possible label length from 1 to 10+ characters (shorter names cost more). `ExchangeRate` holds a single value used to convert between a reference price unit and the native token. These are not queried directly by clients; they feed into the registration fee calculation inside the registrar.

---

## The RPC Methods

There are two types of RPC calls available. The first group (`pns_*`) are full JSON-RPC methods — easy to call from any WebSocket client. The second group is only reachable via `state_call`, which requires encoding your parameters in SCALE format.

---

### JSON-RPC Methods (`pns_` namespace)

#### `pns_getInfo(node: H256)`
**Returns:** full name record, or null

Takes a name hash and returns everything about that name: the owner, expiry, capacity, registration fee, whether it's for sale, and which block the record was read at. Returns null if the name doesn't exist, is expired, or is sitting in the gift waiting room (OfferedNames).

**Reads:** RegistrarInfos, NFT Tokens, Listings, OfferedNames

---

#### `pns_resolveName(name: string)`
**Returns:** full name record, or null

Same as `pns_getInfo` but takes a plain name string like `"alice"` instead of a hash. Computes the hash internally. This is the main call for any UI that starts with a name the user typed.

**Reads:** RegistrarInfos, NFT Tokens, Listings, OfferedNames

---

#### `pns_lookup(node: H256, record_types: number[])`
**Returns:** array of `[record_type_code, bytes]`

Takes a name hash and a list of record type codes (e.g. `[1, 28, 65280]`) and returns all matching DNS records for that name. The SS58 record (65280) is always included in the response when it exists, even if you didn't ask for it.

**Reads:** Records

---

#### `pns_lookupByName(name: string, record_types: number[])`
**Returns:** array of `[record_type_code, bytes]`

Same as `pns_lookup` but takes a plain name string. Computes the hash internally. One round-trip instead of two.

**Reads:** Records

---

#### `pns_getListing(name: string)`
**Returns:** listing record, or null

Checks whether a name is currently for sale. If it is, returns the seller, price, and listing expiry. Returns null if the name has no active listing.

**Reads:** Listings

---

#### `pns_all()`
**Returns:** array of `[name_hash, registrar_info]`

Dumps every registered name and its registration details. This is expensive — it reads the entire RegistrarInfos table. Intended for indexers and block explorers, not for normal app use.

**Reads:** RegistrarInfos (full scan)

---

#### `pns_accountDashboard(account: AccountId)`
**Returns:** dashboard object

Returns the complete name portfolio for an account in a single call, instead of making four separate calls. The response contains:

- **primary_name** — the account's canonical name hash, or null if they have none
- **subnames** — all active subname hashes held by this account
- **pending_subname_offers** — subname hashes offered to this account that haven't been accepted yet
- **pending_name_offers** — top-level name gift hashes waiting for this account to accept

**Reads:** OwnerToPrimaryName, AccountToSubnames, OfferedToAccount, OfferedNames

---

### Runtime API Methods (`state_call`)

These require SCALE-encoding your input. Each method name for `state_call` is `PnsStorageApi_<method_name>`.

---

#### `get_subname(node: H256)`
**Returns:** subdomain record, or null

Returns the full record for a specific subdomain hash: its parent name, its label, the account it belongs to, and whether it's Offered, Active, or Rejected.

**Reads:** SubnameRecords
