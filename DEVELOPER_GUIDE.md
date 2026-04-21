# PNS Node — Developer Guide

This guide covers all extrinsics (state-changing calls) and RPC queries exposed by the PNS node,
along with copy-paste examples using `curl` and `polkadot.js`.

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [How Substrate Extrinsics Work](#how-substrate-extrinsics-work)
3. [RPC Queries (curl)](#rpc-queries-curl)
4. [Registrar Extrinsics](#registrar-extrinsics)
5. [Registry Extrinsics](#registry-extrinsics)
6. [Resolvers Extrinsics](#resolvers-extrinsics)
7. [Marketplace Extrinsics](#marketplace-extrinsics)
8. [Custom DNS Record Types](#custom-dns-record-types)
9. [Key Invariants](#key-invariants)

---

## Prerequisites

Start a local dev node (Alice as the authority, ephemeral state):

```bash
./target/release/solochain-template-node --dev
```

The node listens at:
- **JSON-RPC (HTTP + WS):** `http://localhost:9944`
- **DNS (UDP):** port `5353` (if ddns is enabled)

Install polkadot.js for signing extrinsics:

```bash
npm install @polkadot/api @polkadot/keyring
```

---

## How Substrate Extrinsics Work

Extrinsics are signed transactions. You cannot call them with a plain `curl` POST — the payload must be SCALE-encoded and cryptographically signed offline before submission.

The general flow is:

```
1. Build the call data (pallet index + call index + SCALE-encoded arguments)
2. Fetch the sender's current nonce and the chain's genesis hash / runtime version
3. Sign the payload with the sender's private key
4. Submit the opaque extrinsic bytes via author_submitExtrinsic JSON-RPC
```

All examples below use a small polkadot.js snippet to sign and submit. The `curl` equivalent for extrinsic submission always looks the same — only the hex payload changes:

```bash
curl -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "author_submitExtrinsic",
    "params": ["0x<signed_hex>"],
    "id": 1
  }'
```

To generate `0x<signed_hex>` without polkadot.js, use the **Extrinsics** tab in
[Polkadot-JS Apps](https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9944) — it serialises and signs for you.

---

## RPC Queries (curl)

These are read-only. No signing required.

### `pns_resolveName` — look up a domain by plain label

```bash
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "pns_resolveName",
    "params": ["alice"],
    "id": 1
  }'
```

**Response (found):**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "owner": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "expire": 1773964800000,
    "capacity": 100,
    "register_fee": "1000000000000",
    "for_sale": false
  },
  "id": 1
}
```

**Response (not found / expired):**
```json
{ "jsonrpc": "2.0", "result": null, "id": 1 }
```

---

### `pns_getInfo` — look up a domain by namehash (H256)

```bash
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "pns_getInfo",
    "params": ["0xce159cf34380757d1932a8e4a74e85e85957b0a7a52d9c566c0a3c8d6133d0f7"],
    "id": 1
  }'
```

Returns the same `NameRecord` structure as `pns_resolveName`.

---

### `pns_lookup` — fetch all DNS records for a domain

```bash
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "pns_lookup",
    "params": ["0xce159cf34380757d1932a8e4a74e85e85957b0a7a52d9c566c0a3c8d6133d0f7"],
    "id": 1
  }'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": [
    [65280, "0x..."],
    [1,     "0x..."],
    [28,    "0x..."]
  ],
  "id": 1
}
```

Each entry is `[record_type_code, hex_encoded_bytes]`. See [Custom DNS Record Types](#custom-dns-record-types) for the type codes.

---

### `pns_getListing` — check if a domain is listed for sale

```bash
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "pns_getListing",
    "params": ["alice"],
    "id": 1
  }'
```

**Response (listed):**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "seller": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "price": "5000000000000",
    "expires_at": 1774051200000
  },
  "id": 1
}
```

---

## Registrar Extrinsics

Pallet index **11**. These are the core domain lifecycle calls.

---

### `register` — register a new domain

Caller pays a registration fee (burned). Each account may hold at most one canonical name.

**Call index:** `2`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Plain ASCII label, e.g. `"alice"` |
| `owner` | `AccountId` | Who will own the domain |

**polkadot.js example:**

```js
const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');

async function register() {
  const api = await ApiPromise.create({ provider: new WsProvider('ws://127.0.0.1:9944') });
  const keyring = new Keyring({ type: 'sr25519' });
  const alice = keyring.addFromUri('//Alice');

  // register(name, reject_offer) — owner is always the signing caller.
  // To gift a name to another account, use the marketplace buy-for-recipient
  // or subdomain offer flow; both feed into the recipient-consented
  // `OfferedNames` / `accept_offered_name` path.
  const tx = api.tx.pnsRegistrar.register(
    '0x616c696365', // hex for "alice"
    null            // reject_offer: Option<Vec<u8>>
  );

  const hash = await tx.signAndSend(alice);
  console.log('submitted:', hash.toHex());
  await api.disconnect();
}
register();
```

**Possible errors:** `LabelInvalid`, `ParseLabelFailed`, `RegistrarClosed`, `Frozen` (reserved name), `AlreadyHasCanonicalName`, `Occupied`.

---

### `renew` — renew your canonical domain

Resets expiry to `MaxRegistrationDuration` (365 days) from now. Caller pays renewal fee (burned). Only callable during the active period or within the 30-day grace period.

**Call index:** `3`

No parameters.

```js
const tx = api.tx.pnsRegistrar.renew();
const hash = await tx.signAndSend(alice);
```

**curl (after signing):**
```bash
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"author_submitExtrinsic","params":["0x<signed_hex>"],"id":1}'
```

**Possible errors:** `NoCanonicalName`, `NotRenewable`, `RegistrarClosed`.

---

### `transfer` — transfer your domain to another account

Recipient must not already own a valid canonical name.

**Call index:** `4`

| Parameter | Type | Description |
|---|---|---|
| `to` | `AccountId` | Destination account |

```js
const tx = api.tx.pnsRegistrar.transfer(bob.address);
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `NoCanonicalName`, `NotOwned`, `AlreadyHasCanonicalName` (recipient conflict), `RegistrarClosed`.

---

### `mint_subname` — create a subdomain

The caller must own (or be an approved operator of) the parent domain.

**Call index:** `5`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Either `"sub"` (uses caller's canonical domain) or `"sub.parent"` |
| `to` | `AccountId` | Owner of the new subdomain |

```js
// Creates "dev.alice" if alice is the canonical name of the caller
const tx = api.tx.pnsRegistrar.mintSubname(
  '0x6465762e616c696365', // hex for "dev.alice"
  bob.address
);
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `NoCanonicalName`, `ParseLabelFailed`, `RegistrarClosed`, `CapacityNotEnough`.

---

### `release_name` — release (burn) your canonical domain

Permanently removes your domain NFT and returns the label to the open pool for re-registration.
All subdomains must be deleted before releasing.

**Call index:** `6`

No parameters.

```js
const tx = api.tx.pnsRegistrar.releaseName();
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `NoCanonicalName`, `SubnodeNotClear`.

---

### `add_reserved` / `remove_reserved` — manage reserved names (manager only)

These require the `ManagerOrigin` (root in dev mode).

**Call index:** `0` / `1`

| Parameter | Type | Description |
|---|---|---|
| `node` | `H256` | Namehash of the domain to reserve / unreserve |

```js
// Using sudo in dev mode
const node = '0x...'; // namehash of the label to reserve
const tx = api.tx.sudo.sudo(api.tx.pnsRegistrar.addReserved(node));
const hash = await tx.signAndSend(alice); // Alice is sudo in --dev
```

---

## Registry Extrinsics

Pallet index **10**. These manage NFT ownership and operator approvals.

---

### `approval_for_all` — grant/revoke all-domain operator rights

Lets another account act as an operator on all domains you own.

**Call index:** `0`

| Parameter | Type | Description |
|---|---|---|
| `operator` | `AccountId` | Account to grant or revoke |
| `approved` | `bool` | `true` = grant, `false` = revoke |

```js
const tx = api.tx.pnsRegistry.approvalForAll(bob.address, true);
const hash = await tx.signAndSend(alice);
```

---

### `approve` — grant/revoke per-domain approval

Like ERC-721 approval: lets one specific account manage one specific domain.

**Call index:** `4`

| Parameter | Type | Description |
|---|---|---|
| `to` | `AccountId` | Account to approve or revoke |
| `name` | `Vec<u8>` | Plain label, e.g. `"alice"` or `"dev.alice"` |
| `approved` | `bool` | `true` = grant, `false` = revoke |

```js
const tx = api.tx.pnsRegistry.approve(
  bob.address,
  '0x616c696365', // "alice"
  true
);
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `NoPermission`, `NotExist`, `ApprovalFailure` (cannot approve current owner), `InvalidName`.

---

### `set_resolver` — change the resolver for a domain

**Call index:** `1`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Plain label (`"alice"` or `"dev.alice"`) |
| `resolver` | `AccountId` | New resolver account |

```js
const tx = api.tx.pnsRegistry.setResolver(
  '0x616c696365', // "alice"
  resolverAccount
);
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `NoPermission`, `NotExist`, `InvalidName`.

---

### `burn` — burn a domain NFT

Permanently destroys a domain. All subdomains must already be burned.

**Call index:** `2`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Plain label (`"alice"` or `"dev.alice"`) |

```js
const tx = api.tx.pnsRegistry.burn('0x616c696365');
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `NoPermission`, `NotExist`, `SubnodeNotClear`, `BanBurnBaseNode`, `InvalidName`.

---

### `set_official` — set the root ".dot" official account (manager only)

**Call index:** `3`

| Parameter | Type | Description |
|---|---|---|
| `official` | `AccountId` | New official account that holds the root node NFT |

```js
const tx = api.tx.sudo.sudo(api.tx.pnsRegistry.setOfficial(newOfficial.address));
const hash = await tx.signAndSend(alice);
```

---

## Resolvers Extrinsics

Pallet index **12**. Set DNS records and metadata for domains you own.

---

### `set_account` — attach an address to a domain

Associates a blockchain address (Substrate, Ethereum, Bitcoin, etc.) with your domain.

**Call index:** `0`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Plain label (`"alice"` or `"dev.alice"`) |
| `address` | `Address` | Address enum: `Substrate([u8;32])`, `Ethereum([u8;20])`, `Bitcoin([u8;25])`, or `Id(AccountId)` |

```js
// Attach an Ethereum address
const tx = api.tx.pnsResolvers.setAccount(
  '0x616c696365',  // "alice"
  { Ethereum: '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045' }
);
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `InvalidPermission`, `InvalidName`.

---

### `set_record` — set a DNS record

Sets any DNS record type for a domain. The SS58 owner record (`65280`) is managed by the chain and cannot be overwritten directly.

**Call index:** `1`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Plain label |
| `record_type` | `u32` | DNS type code (see [Custom DNS Record Types](#custom-dns-record-types)) |
| `content` | `Vec<u8>` | Raw record bytes |

```js
// Set an IPv4 A record (type 1)
const ipBytes = new Uint8Array([1, 2, 3, 4]); // 1.2.3.4
const tx = api.tx.pnsResolvers.setRecord(
  '0x616c696365',  // "alice"
  1,               // A record
  ipBytes
);
const hash = await tx.signAndSend(alice);

// Set an RPC endpoint (type 65281)
const encoder = new TextEncoder();
const tx2 = api.tx.pnsResolvers.setRecord(
  '0x616c696365',
  65281,
  encoder.encode('wss://alice.example.com:9944')
);
const hash2 = await tx2.signAndSend(alice);
```

**Possible errors:** `InvalidPermission`, `Ss58RecordProtected`, `InvalidName`.

---

### `set_text` — set metadata text for a domain

**Call index:** `3`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Plain label |
| `kind` | `TextKind` | One of: `Email`, `Url`, `Avatar`, `Description`, `Notice`, `Keywords`, `Twitter`, `Github`, `Ipfs` |
| `content` | `Vec<u8>` | UTF-8 value |

```js
const encoder = new TextEncoder();

const tx = api.tx.pnsResolvers.setText(
  '0x616c696365',              // "alice"
  'Url',                        // TextKind variant
  encoder.encode('https://alice.example.com')
);
const hash = await tx.signAndSend(alice);
```

```js
// Set Twitter handle
const tx = api.tx.pnsResolvers.setText(
  '0x616c696365',
  'Twitter',
  encoder.encode('@alice_dot')
);
```

**Possible errors:** `InvalidPermission`, `InvalidName`.

---

### `remove_account` — remove an address mapping

**Call index:** `4`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Plain label |
| `address` | `Address` | The address to remove |

```js
const tx = api.tx.pnsResolvers.removeAccount(
  '0x616c696365',
  { Ethereum: '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045' }
);
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `InvalidPermission`, `AddressNotFound`, `InvalidName`.

---

## Marketplace Extrinsics

Pallet index **13**. List and buy domain names peer-to-peer.

There is **no escrow** — the listed domain stays fully usable by its owner until sold. If the owner transfers or releases the name before a buyer arrives, the listing becomes stale and `buy_name` will fail.

A **2% protocol fee** is burned from seller proceeds on every sale.

---

### `create_listing` — list your canonical domain for sale

**Call index:** `0`

| Parameter | Type | Description |
|---|---|---|
| `price` | `u128` | Asking price in the smallest currency unit (planck) |
| `expires_at` | `u64` | Millisecond Unix timestamp after which the listing auto-expires |

```js
// List "alice" for 5 DOT (assuming 12 decimals) for 7 days
const price = BigInt('5000000000000');         // 5 * 10^12
const expiresAt = Date.now() + 7 * 86400_000; // 7 days from now (ms)

const tx = api.tx.pnsMarketplace.createListing(price, expiresAt);
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `NoCanonicalName`, `AlreadyListed`, `ExpiryNotInFuture`, `RegistrarClosed`.

---

### `cancel_listing` — cancel your active listing

**Call index:** `1`

No parameters.

```js
const tx = api.tx.pnsMarketplace.cancelListing();
const hash = await tx.signAndSend(alice);
```

**Possible errors:** `NoCanonicalName`, `NotListed`.

---

### `buy_name` — purchase a listed domain

Transfers `price` from buyer to seller, burns 2% from seller proceeds, then atomically transfers the domain NFT to the buyer.

**Call index:** `2`

| Parameter | Type | Description |
|---|---|---|
| `name` | `Vec<u8>` | Plain label of the domain to purchase, e.g. `"alice"` |

```js
const tx = api.tx.pnsMarketplace.buyName('0x616c696365'); // "alice"
const hash = await tx.signAndSend(bob);
```

**Possible errors:** `InvalidName`, `NotListed`, `ListingExpired`, `SellerNoLongerOwns`, `BuyerIsSeller`.

---

### Full Marketplace Flow (polkadot.js)

```js
const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');

async function marketplaceDemo() {
  const api = await ApiPromise.create({ provider: new WsProvider('ws://127.0.0.1:9944') });
  const keyring = new Keyring({ type: 'sr25519' });
  const alice = keyring.addFromUri('//Alice');
  const bob   = keyring.addFromUri('//Bob');

  const aliceName = '0x616c696365'; // "alice"

  // 1. Alice registers "alice" (owner is always the caller; no recipient
  // param — use the offer/accept flow to gift).
  await api.tx.pnsRegistrar.register(aliceName, null)
    .signAndSend(alice, { nonce: -1 });

  // 2. Alice lists for 5 DOT, expires in 7 days
  const price = BigInt('5000000000000');
  const expiresAt = Date.now() + 7 * 86400_000;
  await api.tx.pnsMarketplace.createListing(price, expiresAt)
    .signAndSend(alice, { nonce: -1 });

  // 3. Bob queries the listing
  const listing = await api.rpc.pns.getListing('alice');
  console.log('Listing:', listing.toJSON());

  // 4. Bob buys
  await api.tx.pnsMarketplace.buyName(aliceName)
    .signAndSend(bob, { nonce: -1 });

  // 5. Verify new owner
  const record = await api.rpc.pns.resolveName('alice');
  console.log('New owner:', record.toJSON()?.owner);

  await api.disconnect();
}
marketplaceDemo();
```

---

## Custom DNS Record Types

| Code | Type | Description |
|---|---|---|
| `65280` | SS58 address | Owner's Substrate SS58 address (chain-managed, read-only via `set_record`) |
| `65281` | RPC endpoint | WebSocket or HTTPS RPC URL |
| `65282` | Validator stash | SS58 address of the validator stash |
| `65283` | Parachain ID | Parachain ID as a little-endian u32 |
| `65284` | PROXY | CNAME-equivalent redirect to another domain |
| `65285` | PUBKEY | Public key for encrypted messaging |
| `65286` | AVATAR | IPFS CID (content identifier) |
| `65287` | CONTRACT | Smart contract address |

Standard DNS types are also supported (A=1, AAAA=28, CNAME=5, TXT=16, MX=15, NS=2, SOA=6, etc.).

---

## Key Invariants

| Rule | Details |
|---|---|
| **One canonical name per account** | Registering fails with `AlreadyHasCanonicalName` if you already own a valid name. Call `release_name` first to swap. |
| **Expiry + grace period** | Names expire after 365 days. A 30-day grace period follows — during grace the previous owner may renew but the name resolves as `null`. After grace anyone may register it. |
| **Fee burning** | Registration, renewal, and the marketplace 2% protocol fee are all burned. There is no deposit; fees are not recoverable. |
| **Case-insensitive** | `"Alice"`, `"alice"`, and `"ALICE"` hash identically. Always normalize to lowercase. |
| **Subdomain cap** | Each domain has a fixed `capacity` (default 100). `mint_subname` fails with `CapacityNotEnough` when the cap is reached. |
| **No escrow on listings** | Listing a name does not lock it. Transferring or releasing a listed name makes the listing stale. `buy_name` will return `SellerNoLongerOwns` in that case. |
