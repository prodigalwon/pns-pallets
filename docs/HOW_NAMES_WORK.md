# How PNS Names Work

This document explains all the rules and behaviours of the Dot Naming Service in plain language.

---

## Canonical Names (Top-Level Names)

A **canonical name** is a name like `alice.dot` — a full, top-level name that belongs entirely to one person. Think of it like your username on the whole network.

### Registering

- Alice picks a name and pays the registration fee. The fee is **burned** — it does not go to anyone. It is gone forever.
- Her name is valid for exactly **365 days** from the moment she registers it.
- She can only ever hold **one** canonical name at a time. If she already has one, she cannot register another until she gives up the one she has.

### Fees and Name Length

Shorter names cost more than longer ones. The price is set by the network manager and depends on how many characters are in the name:

- A 1-character name has a different price than a 2-character name, a 3-character name, and so on up to 6+ characters.
- A 6-character name and an 11-character name cost the same.
- The fee is the same whether you are registering or renewing.

### Case Does Not Matter

`Alice`, `alice`, and `ALICE` are all the same name. Everything is converted to lowercase internally.

### Renewing

- Alice can renew her name before it expires (or during the grace period — more on that below).
- Renewal does **not** add time on top of what she has left. It resets her expiry to a full **365 days from right now**.
- She can never have more than 365 days on the clock. There is no stacking.

### Expiry and the Grace Period

- When her name expires, it enters a **30-day grace period**.
- During the grace period, her name is **hers but broken** — nobody can look her up by it. It returns nothing to anyone querying it.
- She is still the only one who can renew it during this window.
- If she renews during grace, everything comes back to life immediately.
- If the grace period ends without a renewal, the name is **fully released** and anyone can register it.

### Releasing a Name

Alice can voluntarily give up her name at any time with `release_name`. This frees her to register a new one immediately.

---

## Transferring a Name

Alice can send her name directly to Bob using `transfer`. Bob receives it as his canonical name. The usual rules apply — Bob cannot already have a canonical name of his own, or the transfer will fail.

---

## The Marketplace

Alice can list her name for sale on the marketplace.

- She sets a price and an expiry date for the listing.
- Anyone can buy it before the listing expires.
- When a sale happens, **2% of the sale price is burned** as a protocol fee. Alice receives the rest.
- The buyer cannot already have a canonical name.
- The buyer can optionally designate someone else as the recipient — they buy it as a gift. The gift recipient then receives an offer they must accept (same as how subdomain offers work below).
- If Alice transfers or releases her name before anyone buys it, the listing becomes invalid but is not automatically removed. It simply cannot be used.

---

## Subdomains

A subdomain is a name like `charlie.alice.dot`. Alice (owner of `alice.dot`) creates it and offers it to someone. It is a second-level name under her root name.

### Offering a Subdomain

- Alice can offer subdomains to other accounts using `offer_subdomain`.
- She picks the label (e.g. `charlie`) and the target account. The offer is created in a **pending** state.
- Alice cannot offer a subdomain to herself.
- The target cannot already own a canonical name or any other subdomain — one name per account, always.

### Accepting

- Charlie receives the offer and must explicitly accept it with `accept_subdomain`.
- Until Charlie accepts, the subdomain exists in storage but is not active. It cannot be looked up.
- Once Charlie accepts, the subdomain becomes active and Charlie is the holder.

### Rejecting

- Charlie can reject the offer with `reject_subdomain` instead. The record flips to `Rejected` state.
- Alice must then call `revoke_subdomain` to clean it up and reclaim the capacity slot.

### Capacity — The 10-Slot Limit

- Every canonical name comes with **10 subdomain slots**.
- Each time Alice sends an offer, it **immediately consumes one slot**, even before the target accepts.
- Accepting does not free the slot. Only revoking or releasing does.
- Once all 10 slots are used up, Alice cannot send any more offers until she revokes some.

This is intentional to prevent spam — it makes sending offers carry a real cost in attention and chain operations, not just fees.

### Example

Bob sends 10 subdomain offers to 10 different people. His capacity is now 0. Charlie accepts `charlie.bob.dot`. That slot is now **Active**, but Bob still has 0 free slots — acceptance does not release capacity. Bob must call `revoke_subdomain` on one of the other 9 pending offers to free a slot and send a new offer. He has to remember what names he offered, because there is no automatic cleanup.

### Subdomains Cannot Be Sold or Transferred

- Charlie cannot list `charlie.bob.dot` on the marketplace. The marketplace only works with canonical (top-level) names.
- Charlie cannot transfer `charlie.bob.dot` to anyone else. There is no such extrinsic.
- If Charlie wants out, he calls `release_subname`. That frees the slot back to Bob and deletes the record.
- Bob can forcibly take it back at any time with `revoke_subdomain`.

### Subdomain Depth

Subdomains can only be one level deep. You cannot create a subdomain under a subdomain. `x.charlie.alice.dot` is not possible.

---

## Reserved Names

The following names can never be registered by anyone. They were reserved at genesis at the request of the Polkadot Technical Fellowship:

```
polkadot
kusama
paseo
westend
fellowship
hub
polkadothub
assethub
collectives
pusd
pop
revive
jam
people
dap
```

The network manager can add more reserved names after launch using `add_reserved`, and can remove them with `remove_reserved`.

---

## One Name Per Account — Always

This rule applies everywhere:

- You cannot register a canonical name if you already have one.
- You cannot accept a subdomain offer if you already have a canonical name.
- You cannot accept a subdomain offer if you already have a different subdomain.
- You cannot receive a transferred name or a gifted name if you already hold any name.

The only way to get a new name is to give up the one you have first.

---

## Summary Table

| Action | Who can do it | Notes |
|---|---|---|
| Register | Anyone without an active name | Fee burned, 365-day max |
| Renew | Name holder (including during grace) | Resets to 365 days, not additive |
| Transfer | Name holder | Recipient must be nameless |
| Release | Name holder | Immediate, clears the slot |
| Create listing | Name holder | Price + expiry; 2% fee burned on sale |
| Cancel listing | Name holder | Removes listing, name stays |
| Buy name | Anyone without a name | Cannot be the seller |
| Offer subdomain | Canonical name holder | Consumes a capacity slot immediately |
| Accept subdomain | Offer target | Must be nameless |
| Reject subdomain | Offer target | Sets state to Rejected; offerer must revoke to clean up |
| Revoke subdomain | Parent domain owner | Works in any state; frees the capacity slot |
| Release subdomain | Subdomain holder | Voluntary exit; frees the capacity slot |
