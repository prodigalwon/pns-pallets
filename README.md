# Polkadot Name Service (PNS)

A native DNS and identity system for the Polkadot ecosystem, implemented as a set of FRAME pallets on a solochain node built with polkadot-sdk stable2603.

PNS maps human-readable names like `frank.dot` to Polkadot addresses, validator nodes, RPC endpoints, parachains, and public keys — making the network accessible without copying 48-character SS58 addresses.

## Vision

Polkadot has world-class infrastructure but a UX problem. Sending funds, staking to a validator, or connecting to an RPC endpoint all require handling long opaque addresses. PNS solves this at the protocol level by bringing DNS-style naming directly on-chain.

A registered name like `rico.dot` can resolve to:
- An SS58 wallet or account address
- A validator stash address
- A WebSocket RPC endpoint (`wss://rico.dot:9944`)
- A parachain ID
- A public key for encrypted peer-to-peer messaging
- An IPFS avatar/profile image
- A smart contract address (ink!/Wasm or EVM)
- Up to three raw public key slots for post-quantum or application-defined key material

## Architecture

| Crate | Description |
|---|---|
| `pns-types` | Shared types: `Record`, `RegistrarInfo`, `NameRecord`, DNS schema, `MaxPubKeySize` bound |
| `pns-registrar` | Domain registration, renewal, expiry, fee burn, and price oracle |
| `pns-resolvers` | DNS record storage and lookup |
| `pns-marketplace` | On-chain name marketplace — list, cancel, and atomically purchase names |
| `pns-runtime-api` | Runtime API trait (`get_info`, `resolve_name`, `lookup`, `all`) |
| `pns-ddns` | Axum HTTP REST API for off-chain name resolution (`GET /info/:name`, `GET /get_info/:id`, `GET /all`) |
| `solochain-template-node` | Substrate node binary with custom PNS JSON-RPC endpoints |
| `solochain-template-runtime` | FRAME runtime wiring all pallets together |

## Record Types

PNS extends standard DNS with Polkadot-native record types in the IANA private use range (65280–65292):

| Type | Code | Description |
|---|---|---|
| `SS58` | 65280 | Polkadot SS58 encoded address |
| `RPC` | 65281 | WebSocket RPC endpoint |
| `VALIDATOR` | 65282 | Validator stash address |
| `PARA` | 65283 | Parachain ID |
| `PROXY` | 65284 | PNS name pointer (CNAME equivalent) |
| `PUBKEY1` | 65285 | Public key slot 1 (up to 1024 bytes; accommodates post-quantum keys) |
| `AVATAR` | 65286 | IPFS CID of the owner's profile image — store the raw CID string (e.g. `bafybeig...`) |
| `CONTRACT` | 65287 | Smart contract address (ink!/Wasm or EVM) |
| `PUBKEY2` | 65288 | Public key slot 2 |
| `PUBKEY3` | 65289 | Public key slot 3 |
| `ORIGIN` | 65290 | 32-byte block hash of the original registration — on-chain proof of purchase |
| `IPFS` | 65291 | TLS certificate public key for this domain — allows clients to verify TLS without a traditional CA chain |
| `CONTENT` | 65292 | IPFS CID of a website or dapp — store the raw CID string (e.g. `bafybeig...`); browsers and gateways resolve this to serve the site |

Standard DNS record types (A, AAAA, CNAME, TXT, MX, NS, etc.) are also supported.

## Economics

- Registration and renewal fees are **burned** — permanently removed from total supply via `Currency::withdraw` drop
- No deposit — one flat fee per registration or renewal
- Names expire after a maximum of 365 days (in milliseconds via `pallet-timestamp`); this will migrate to era-based time in a future version
- After expiry, the previous owner has a **30-day grace period** to renew before the name returns to the open pool
- During the grace period (and after), `pns_resolveName` and `pns_getInfo` return `null` — the name is considered dead from a resolution standpoint
- Renewal resets expiry to 365 days from now (not additive)
- An owner may voluntarily **release** their name at any time via `release_name` — the NFT is burned immediately and the name returns to the open pool with no grace period
- Owners may list their name for sale via `create_listing(price, expires_at)`; a 2% protocol fee is burned from the seller's proceeds on sale
- Pricing by label length (default genesis values, adjustable by governance):

| Label length | Fee |
|---|---|
| 1 character | 1000 DOT |
| 2 characters | 100 DOT |
| 3 characters | 45 DOT |
| 4 characters | 25 DOT |
| 5 characters | 10 DOT |
| 6+ characters | 0.5 DOT |

Prices are stored in the `PnsPriceOracle` pallet's `BasePrice` storage entry and can be updated by an admin at any time. **Client applications are responsible for querying the current price before submitting a `register` or `renew` extrinsic.** The node does not accept a fee parameter — it computes and charges the fee entirely on-chain at execution time. Apps should read `BasePrice` (or call `PnsStorageApi` equivalents) immediately before building the transaction so the user sees the live price, and warn if the price has changed since the user's last query.

## JSON-RPC API

The node exposes custom PNS endpoints under the `pns_` namespace:

| Method | Parameters | Returns |
|---|---|---|
| `pns_getInfo` | `node: H256` (namehash) | `NameRecord \| null` |
| `pns_resolveName` | `name: String` (e.g. `"alice"`) | `NameRecord \| null` |
| `pns_lookup` | `node: H256` (namehash) | `Array<[recordType: u32, data: Bytes]>` |
| `pns_getListing` | `name: String` (e.g. `"alice"`) | `ListingInfo \| null` |

`pns_resolveName` computes the namehash internally against the native base node (`.dot`), so callers pass a plain label string rather than a raw hash. Resolution is **case-insensitive** — `"Alice"`, `"alice"`, and `"ALICE"` all resolve identically.

Both `pns_getInfo` and `pns_resolveName` return `null` if the name does not exist **or has expired**. A `null` response means the name is available (or in its grace period and available for renewal only).

### Checking name availability

To check whether a name is already registered, call `pns_resolveName`. A non-null result means the name is active and taken; `null` means it is available to register:

```json
{"jsonrpc":"2.0","id":1,"method":"pns_resolveName","params":["alice"]}
```

### Runtime API (`state_call`)

All methods above delegate to the `PnsStorageApi` runtime API, which can also be called directly via `state_call`. This is useful for light clients, subxt integrations, or any context where you need a verifiable on-chain result:

| Runtime API function | `state_call` method name |
|---|---|
| `resolve_name(name: Vec<u8>)` | `PnsStorageApi_resolve_name` |
| `get_info(id: DomainHash)` | `PnsStorageApi_get_info` |
| `lookup(id: DomainHash)` | `PnsStorageApi_lookup` |
| `lookup_by_name(name: Vec<u8>)` | `PnsStorageApi_lookup_by_name` |
| `name_to_hash(name: Vec<u8>)` | `PnsStorageApi_name_to_hash` |
| `get_listing(name: Vec<u8>)` | `PnsStorageApi_get_listing` |
| `primary_name(owner: AccountId)` | `PnsStorageApi_primary_name` |
| `subnames_of(owner: AccountId)` | `PnsStorageApi_subnames_of` |

### Reverse lookup

To check whether a signed-in account has a canonical name, call `primary_name` with their SS58 `AccountId`. Returns the `DomainHash` of their primary name, or `null` if they have none. Pass the returned hash to `get_info` to get the full `NameRecord`:

```json
{"jsonrpc":"2.0","id":1,"method":"state_call","params":["PnsStorageApi_primary_name","<SCALE-encoded AccountId>"]}
```

To enumerate all subnames held by an account, call `subnames_of`. Returns an array of `DomainHash` values. Pass each to `get_info` or `lookup` for details:

```json
{"jsonrpc":"2.0","id":1,"method":"state_call","params":["PnsStorageApi_subnames_of","<SCALE-encoded AccountId>"]}
```

### Subname delegation

Subdomains use an explicit offer/accept flow. They are not NFTs — ownership is tracked via `SubnameRecord` storage in the registry pallet.

**States:** `Offered` → `Active` (accepted) or `Rejected` (declined). Records in `Offered` or `Rejected` state are visible to the offerer. The record is deleted on revoke or release.

**Extrinsics:**

| Extrinsic | Who can call | What it does |
|---|---|---|
| `offer_subdomain(label, target)` | Parent name owner only | Creates a `SubnameRecord` in `Offered` state addressed to `target` |
| `accept_subdomain(parent, label)` | `target` account only | Flips state to `Active`, writes SS58 record |
| `reject_subdomain(parent, label)` | `target` account only | Flips state to `Rejected` so offerer can see the outcome |
| `revoke_subdomain(label)` | Parent name owner only | Deletes the record regardless of state; clears DNS if Active |
| `release_subdomain(parent, label)` | Active holder only | Deletes an Active record voluntarily; clears DNS |

**Invariants:**
- Depth is capped at one level — `sub.canonical.dot` is valid; `deep.sub.canonical.dot` is not.
- The canonical name owner cannot be the `target` of their own subdomain offer.
- Subdomains have no independent expiry. They inherit the parent's expiry from `RegistrarInfos`. When the parent expires, is transferred, or is released, all subdomains are cleared atomically.

**Runtime API for subdomains:**

| Function | Returns |
|---|---|
| `get_subname(node)` | `SubnameRecord \| null` — parent, label, target, state |
| `pending_offers_for(account)` | `Array<DomainHash>` — offers awaiting acceptance |
| `subnames_of(account)` | `Array<DomainHash>` — active subnames held by account |

Example — resolve `"alice"` via `state_call` (param is SCALE-encoded `Vec<u8>`):

```json
{"jsonrpc":"2.0","id":1,"method":"state_call","params":["PnsStorageApi_resolve_name","0x05616c696365"]}
```

The `0x05` prefix is the SCALE compact-encoded length (5 bytes), followed by `616c696365` (`"alice"` in hex).

### Exploring the API with subxt

With the metadata file, view functions (e.g. `lookup` on `PnsResolvers`) and runtime APIs can be explored via the subxt CLI:

```bash
# List PnsResolvers storage entries
subxt explore --file rust_core/src/polkadot_metadata.scale pallet PnsResolvers storage

# Explore the lookup view function
subxt explore --file rust_core/src/polkadot_metadata.scale pallet PnsResolvers view_functions lookup

# Explore the PnsStorageApi runtime API
subxt explore --file rust_core/src/polkadot_metadata.scale runtime_apis PnsStorageApi resolve_name
```

`NameRecord` response shape:

```json
{
  "owner": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
  "expire": 1734567890000,
  "capacity": 10,
  "deposit": 0,
  "register_fee": 500000000000
}
```

`owner` is the SS58-encoded address of the current name holder. `expire` is a Unix millisecond timestamp.

## Record Schema

Each name's NFT token data (`Record`) carries:

```rust
pub struct Record {
    pub children: u32,                                  // number of registered subnames
    pub pubkey_1: Option<BoundedVec<u8, MaxPubKeySize>>, // key slot 1 (max 1024 bytes)
    pub pubkey_2: Option<BoundedVec<u8, MaxPubKeySize>>, // key slot 2 (max 1024 bytes)
    pub pubkey_3: Option<BoundedVec<u8, MaxPubKeySize>>, // key slot 3 (max 1024 bytes)
}
```

`MaxPubKeySize = ConstU32<1024>` is sized to accommodate 800-byte post-quantum public keys (e.g. CRYSTALS-Kyber512) with headroom.

## Building

```bash
cargo check
cargo build --release
```

## Running a dev node

```bash
./target/release/solochain-template-node --dev
```

Then connect via [Polkadot.js Apps](https://polkadot.js.org/apps) pointing to `ws://127.0.0.1:9944`, or locally:

```bash
npx @polkadot/apps
```

## Roadmap

- [x] Migrate to polkadot-sdk stable2603
- [x] Polkadot-native DNS record types (SS58, RPC, VALIDATOR, PARA, PROXY, PUBKEY, AVATAR, CONTRACT)
- [x] Flat fee model — registration fee burned, no deposit
- [x] Tiered pricing by label length
- [x] `fee` field in `NameRegistered` and `NameRenewed` events
- [x] Wire PNS pallets into solochain node and runtime
- [x] Custom JSON-RPC endpoints (`pns_getInfo`, `pns_resolveName`, `pns_lookup`, `pns_getListing`)
- [x] `PnsStorageApi` runtime API — `resolve_name`, `lookup_by_name`, `name_to_hash`, `get_listing`, `primary_name`, `subnames_of` all callable via `state_call`
- [x] Subname inverse index (`AccountToSubnames`) — reverse lookup of all subnames held by an account
- [x] Subname ownership invariant — canonical name owner cannot hold a subname under their own domain
- [x] Case-insensitive name registration and resolution
- [x] One canonical name per address — a second registration requires releasing the first
- [x] `NameRecord` response includes owner SS58 address
- [x] Expired names resolve to `null`; 30-day grace period for renewal before re-registration opens
- [x] Post-quantum key slots on `Record` (`pubkey_1`, `pubkey_2`, `pubkey_3`)
- [x] `release_name` extrinsic — voluntary early burn returns name to open pool immediately
- [x] `pns-marketplace` pallet — `create_listing`, `cancel_listing`, `buy_name` with 2% protocol burn
- [ ] Era-based time (replace millisecond `Moment` with Polkadot era units)
- [ ] Genesis reserved name list
- [ ] Extrinsic benchmarks and proper `WeightInfo`
- [ ] Deploy to Paseo testnet
- [ ] Deploy to Kusama
- [ ] Polkadot OpenGov referendum

## License

Unlicense — public domain
