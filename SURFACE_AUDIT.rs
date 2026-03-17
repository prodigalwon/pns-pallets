// ============================================================================
// PNS NODE — COMPLETE ATTACK SURFACE AUDIT
// Extrinsics + RPC/Runtime-API Methods
// ============================================================================
//
// SCOPE: Every user-callable extrinsic and every read path exposed via the
//        custom pns_ JSON-RPC namespace and the PnsStorageApi runtime API.
//        For each entry: function name, parameters (name, type,
//        mandatory/optional), and the exact storage tables read and/or written.
//
// NOTATION
//   [M] = Mandatory parameter
//   [O] = Optional parameter (Option<T> or derived from caller's canonical name)
//   READ  = storage table(s) accessed for reads during execution
//   WRITE = storage table(s) mutated during execution
//
// PALLET INDICES (construct_runtime! order)
//   8  = PnsNft          (no user extrinsics)
//   9  = PnsPriceOracle  (no user extrinsics)
//  10  = PnsRegistry
//  11  = PnsRegistrar
//  12  = PnsResolvers
//  13  = PnsMarketplace
//
// ============================================================================
// STORAGE TABLE REFERENCE
// ============================================================================
//
// pns-nft (backing layer, pallet index 8)
//   nft::Classes          StorageMap  < ClassId → ClassInfo{ owner, data } >
//   nft::Tokens           StorageDoubleMap < (ClassId, DomainHash) → TokenInfo{ owner, data: Record{ children } } >
//   nft::TokensByOwner    StorageDoubleMap < (AccountId, ClassId), DomainHash → () >
//
// pns-registrar::registry (PnsRegistry, pallet index 10)
//   registry::RuntimeOrigin     StorageMap  < DomainHash → DomainTracing{ Root | RuntimeOrigin(DomainHash) } >
//   registry::SubNames           StorageDoubleMap < DomainHash(root), DomainHash(child) → () >
//   registry::AccountToSubnames  StorageDoubleMap < AccountId, DomainHash → () >
//   registry::SubnameRecords     StorageMap  < DomainHash → SubnameRecord{ parent, label, target, state } >
//   registry::OfferedToAccount   StorageDoubleMap < AccountId, DomainHash → () >
//   registry::Official           StorageValue < AccountId >
//
// pns-registrar::registrar (PnsRegistrar, pallet index 11)
//   registrar::RegistrarInfos    StorageMap  < DomainHash → RegistrarInfo{ expire, capacity, register_fee, label_len, last_block } >
//   registrar::ReservedList      StorageMap  < DomainHash → () >
//   registrar::OwnerToPrimaryName StorageMap < AccountId → DomainHash >
//   registrar::OfferedNames      StorageMap  < DomainHash → OfferedNameRecord{ buyer: AccountId, recipient: AccountId } >
//
// pns-resolvers (PnsResolvers, pallet index 12)
//   resolvers::Records    StorageDoubleMap < DomainHash, RecordType → BoundedVec<u8, MaxContentLen> >
//   resolvers::Texts      StorageDoubleMap < DomainHash, TextKind   → BoundedVec<u8, MaxContentLen> >
//
// pns-marketplace (PnsMarketplace, pallet index 13)
//   marketplace::Listings StorageMap < DomainHash → Listing{ seller, price, expires_at } >
//
// ============================================================================
// RECORD TYPES (resolvers::Records key space)
// ============================================================================
//
// Standard DNS (subset):
//   A, AAAA, ANAME, CNAME, MX, NS, PTR, SOA, SRV, SSHFP, TXT, CAA,
//   CDS, CDNSKEY, DNSKEY, DS, HINFO, HTTPS, KEY, NAPTR, NSEC, NSEC3,
//   NSEC3PARAM, NULL, OPENPGPKEY, OPT, PTR, RRSIG, SIG, SVCB, TLSA, TXT, ZERO
//
// PNS custom (IANA private-use range):
//   SS58     = 65280  PROTECTED — chain-managed; set on register/transfer/buy
//   RPC      = 65281  RPC endpoint URL
//   VALIDATOR= 65282  Validator stash address
//   PARA     = 65283  Parachain ID
//   PROXY    = 65284  CNAME-equivalent delegation
//   PUBKEY1  = 65285  Public key slot 1 (≤2048 bytes; post-quantum capable)
//   AVATAR   = 65286  IPFS CID for avatar image
//   CONTRACT = 65287  Smart-contract address
//   PUBKEY2  = 65288  Public key slot 2
//   PUBKEY3  = 65289  Public key slot 3
//   ORIGIN   = 65290  PROTECTED — block hash of initial registration (chain-managed)
//   IPFS     = 65291  IPFS content link
//   CONTENT  = 65292  Arbitrary content attribute
//
// TEXT KINDS (resolvers::Texts key space):
//   Email, Url, Avatar, Description, Notice, Keywords, Twitter, Github, Ipfs
//
// ============================================================================
// SECTION 1 — EXTRINSICS
// ============================================================================
//
// ----------------------------------------------------------------------------
// PALLET: PnsRegistry (index 10)
// ----------------------------------------------------------------------------
//
// [10.2] burn
//   Description : Destroy a domain NFT token by name string.
//                 Top-level burn auto-clears all subdomains first.
//                 Subdomain burn requires children == 0.
//   Caller      : Must be the NFT token owner of the named domain.
//   Parameters  :
//     name  [M]  Vec<u8>   Plain label ("alice") or dotted name ("sub.alice").
//                          Accepts both "domain" and "sub.domain" forms.
//   READ  : nft::Tokens, registry::RuntimeOrigin, registry::SubNames,
//           registry::SubnameRecords, registrar::RegistrarInfos
//   WRITE : nft::Tokens, nft::TokensByOwner,
//           registry::RuntimeOrigin, registry::SubNames,
//           registry::SubnameRecords, registry::OfferedToAccount,
//           registry::AccountToSubnames,
//           registrar::RegistrarInfos,
//           resolvers::Records, resolvers::Texts
//   NOTES : Restricted to caller == NFT owner.
//           Top-level burn cascades (calls clear_subnames internally).
//           Subdomain burn blocked if children > 0.
//
// [10.3] set_official
//   Description : Update the PNS official (admin) account.
//   Caller      : ManagerOrigin only (sudo / governance).
//   Parameters  :
//     official  [M]  AccountId   New official account.
//   READ  : registry::Official, nft::Tokens
//   WRITE : registry::Official, nft::Tokens, nft::TokensByOwner
//   NOTES : Also transfers the TLD root NFT from old official to new.
//           Only callable by the privileged ManagerOrigin.
//
// ----------------------------------------------------------------------------
// PALLET: PnsRegistrar (index 11)
// ----------------------------------------------------------------------------
//
// [11.0] add_reserved
//   Description : Add a name to the reserved list, preventing public registration.
//   Caller      : ManagerOrigin only (sudo / governance).
//   Parameters  :
//     name  [M]  Vec<u8>   Plain label (e.g. b"polkadot").
//   READ  : (none — hash computed from name, then checked implicitly by insert)
//   WRITE : registrar::ReservedList
//   NOTES : Reserved names cannot be registered by anyone until removed.
//
// [11.1] remove_reserved
//   Description : Remove a name from the reserved list.
//   Caller      : ManagerOrigin only (sudo / governance).
//   Parameters  :
//     name  [M]  Vec<u8>   Plain label.
//   READ  : (none)
//   WRITE : registrar::ReservedList
//
// [11.2] register
//   Description : Register a new top-level name under the native TLD.
//                 Optionally rejects a pending gift offer before registering.
//   Caller      : Any signed account (registrar must be open).
//   Parameters  :
//     name          [M]  Vec<u8>        Plain label to register (e.g. b"alice").
//     owner         [M]  MultiAddress   Destination account for the name.
//                                       Encoded as AccountId (32 bytes) or index.
//     reject_offer  [O]  Option<Vec<u8>> If Some(name_to_reject): reject a pending
//                                        top-level or subdomain offer addressed to
//                                        the CALLER before registering. Caller must
//                                        be the recipient of that offer.
//   READ  : registrar::ReservedList, registrar::OwnerToPrimaryName,
//           registrar::RegistrarInfos, registry::AccountToSubnames,
//           registrar::OfferedNames (if reject_offer is Some),
//           nft::Tokens, registry::RuntimeOrigin
//   WRITE : registrar::OwnerToPrimaryName, registrar::RegistrarInfos,
//           registrar::OfferedNames (if reject_offer is Some — removes entry),
//           nft::Tokens, nft::TokensByOwner,
//           registry::RuntimeOrigin, registry::AccountToSubnames,
//           resolvers::Records (SS58 record, ORIGIN record)
//   NOTES : Fails with AlreadyHasCanonicalName if `owner` already has a valid name.
//           Fails with AlreadyHoldsSubdomain if `owner` holds an active subdomain.
//           Fails with Frozen if name is in ReservedList.
//           Fails with ParseLabelFailed if label contains illegal characters.
//           Fee is burned (non-refundable). No deposit.
//           If reject_offer=Some points to a top-level offered name, the NFT
//           is burned and the slot is freed (recipient loses the gift).
//           If reject_offer=Some points to a subdomain offer, the record is
//           fully revoked (children counter decremented on parent).
//
// [11.3] renew
//   Description : Renew the caller's canonical name for another registration period.
//   Caller      : Must own a canonical name still within the renewable window
//                 (i.e. not yet past expire + GracePeriod).
//   Parameters  : (none — name derived from OwnerToPrimaryName[caller])
//   READ  : registrar::OwnerToPrimaryName, registrar::RegistrarInfos
//   WRITE : registrar::RegistrarInfos  (updates expire field)
//   NOTES : Renewal fee is burned. Expiry is reset to now + MaxRegistrationDuration
//           (not additive). Fails with NoCanonicalName if caller has no name.
//           Fails with NotRenewable if past grace period.
//
// [11.4] transfer
//   Description : Transfer the caller's canonical name to another account.
//   Caller      : Must own a canonical name that is not expired (including grace).
//   Parameters  :
//     to  [M]  MultiAddress   Recipient account.
//   READ  : registrar::OwnerToPrimaryName, registrar::RegistrarInfos,
//           registry::RuntimeOrigin, nft::Tokens, registry::AccountToSubnames
//   WRITE : registrar::OwnerToPrimaryName,
//           nft::Tokens, nft::TokensByOwner,
//           registry::SubNames, registry::SubnameRecords,
//           registry::OfferedToAccount, registry::AccountToSubnames,
//           registry::RuntimeOrigin,
//           resolvers::Records, resolvers::Texts
//   NOTES : Fails with AlreadyHasCanonicalName if `to` already holds a valid name.
//           Fails with AlreadyHoldsSubdomain if `to` holds an active subdomain.
//           Clears all subnames on the name before transferring (clear_subnames).
//           Also clears non-SS58 DNS records; writes new SS58 + ORIGIN records.
//
// [11.5] offer_subdomain
//   Description : Offer a subdomain under the caller's canonical name to a target account.
//   Caller      : Must own a canonical name that is currently live (not expired).
//   Parameters  :
//     label   [M]  Vec<u8>        ASCII label for the subdomain (e.g. b"bob").
//     target  [M]  MultiAddress   Account to receive the offer.
//   READ  : registrar::OwnerToPrimaryName, registrar::RegistrarInfos,
//           registry::RuntimeOrigin, nft::Tokens, registry::SubnameRecords
//   WRITE : nft::Tokens (increments parent children count),
//           registry::SubnameRecords, registry::OfferedToAccount,
//           registry::SubNames
//   NOTES : Depth limited to 1 (parent must be a Root domain, not itself a subdomain).
//           Target cannot be the owner of the parent domain.
//           Fails with CapacityNotEnough if parent already has 10 subdomains.
//           Fails with SubnameAlreadyExists if the label is already used.
//           Creates SubnameRecord in Offered state.
//
// [11.6] release_name
//   Description : Burn the caller's canonical name, returning it to the open pool.
//   Caller      : Must own a canonical name.
//   Parameters  : (none — name derived from OwnerToPrimaryName[caller])
//   READ  : registrar::OwnerToPrimaryName, nft::Tokens,
//           registry::RuntimeOrigin, registry::SubNames, registry::SubnameRecords
//   WRITE : registrar::OwnerToPrimaryName, registrar::RegistrarInfos,
//           nft::Tokens, nft::TokensByOwner,
//           registry::RuntimeOrigin, registry::SubNames,
//           registry::SubnameRecords, registry::OfferedToAccount,
//           registry::AccountToSubnames,
//           resolvers::Records, resolvers::Texts
//   NOTES : Auto-clears all subdomains before burning (clear_subnames).
//           After release caller may register a new canonical name.
//           Fee is NOT refunded (it was burned at registration time).
//
// [11.7] accept_subdomain
//   Description : Accept a pending subdomain offer; activates the subdomain.
//   Caller      : Must be the `target` account in the SubnameRecord.
//   Parameters  :
//     parent  [M]  Vec<u8>   Plain label of the parent domain (e.g. b"alice").
//     label   [M]  Vec<u8>   Plain label of the subdomain (e.g. b"bob").
//   READ  : registrar::RegistrarInfos, registry::SubnameRecords
//   WRITE : registry::SubnameRecords (state: Offered → Active),
//           registry::OfferedToAccount (removes entry),
//           registry::AccountToSubnames (inserts entry),
//           resolvers::Records (writes SS58 record)
//   NOTES : Fails with TargetAlreadyOwnsName if caller already holds any name.
//           Fails with SubnameNotOffered if record is not in Offered state.
//           Fails with NotSubnameTarget if caller is not the designated target.
//           Parent expiry is checked — fails with NotUseable if parent expired.
//
// [11.8] reject_subdomain
//   Description : Reject a pending subdomain offer; marks the record as Rejected.
//   Caller      : Must be the `target` account in the SubnameRecord.
//   Parameters  :
//     parent  [M]  Vec<u8>   Plain label of the parent domain.
//     label   [M]  Vec<u8>   Plain label of the subdomain.
//   READ  : registry::SubnameRecords
//   WRITE : registry::SubnameRecords (state: Offered → Rejected),
//           registry::OfferedToAccount (removes entry)
//   NOTES : Record is NOT deleted — offerer must call revoke_subdomain to clean up.
//           Fails with SubnameNotOffered, NotSubnameTarget.
//
// [11.9] revoke_subdomain
//   Description : Revoke a subdomain (any state) by the parent domain owner.
//   Caller      : Must own the canonical name that is the parent.
//   Parameters  :
//     label  [M]  Vec<u8>   Plain label of the subdomain to revoke.
//   READ  : registrar::OwnerToPrimaryName, registry::SubnameRecords,
//           nft::Tokens
//   WRITE : registry::SubnameRecords (removed),
//           registry::OfferedToAccount (if state was Offered/Rejected),
//           registry::AccountToSubnames (if state was Active),
//           registry::SubNames (removes entry),
//           nft::Tokens (decrements children count),
//           resolvers::Records, resolvers::Texts (if state was Active)
//   NOTES : Works on Offered, Rejected, and Active states.
//           Fails with SubnameNotFound if label doesn't exist under caller's name.
//
// [11.10] release_subdomain
//   Description : Voluntarily release an active subdomain by its holder.
//   Caller      : Must be the `target` account in the SubnameRecord (state Active).
//   Parameters  :
//     parent  [M]  Vec<u8>   Plain label of the parent domain.
//     label   [M]  Vec<u8>   Plain label of the subdomain.
//   READ  : registry::SubnameRecords
//   WRITE : registry::SubnameRecords (removed),
//           registry::AccountToSubnames (removes entry),
//           registry::SubNames (removes entry),
//           nft::Tokens (decrements children count),
//           resolvers::Records, resolvers::Texts
//   NOTES : Fails with SubnameNotActive if not in Active state.
//           Fails with NotSubnameTarget if caller is not the holder.
//
// [11.11] accept_offered_name
//   Description : Accept a top-level name that was purchased as a marketplace gift.
//                 Activates the name: sets OwnerToPrimaryName, writes SS58 + ORIGIN.
//   Caller      : Must be the `recipient` in the OfferedNames record.
//   Parameters  :
//     name  [M]  Vec<u8>   Plain label of the offered name (e.g. b"bob").
//   READ  : registrar::OfferedNames, registrar::RegistrarInfos,
//           registrar::OwnerToPrimaryName, registry::AccountToSubnames
//   WRITE : registrar::OfferedNames (removed),
//           registrar::OwnerToPrimaryName (inserts caller → node),
//           resolvers::Records (writes SS58, ORIGIN)
//   NOTES : Fails with OfferedNameNotFound if no pending offer exists.
//           Fails with NotOfferedNameRecipient if caller is not the intended recipient.
//           Fails with AlreadyHasCanonicalName if caller already owns another name.
//           Fails with AlreadyHoldsSubdomain if caller holds an active subdomain.
//           Fails with NotUseable if the name's registration has expired.
//
// ----------------------------------------------------------------------------
// PALLET: PnsResolvers (index 12)
// ----------------------------------------------------------------------------
//
// [12.1] set_record
//   Description : Write a DNS record for a domain the caller owns or holds.
//   Caller      : Must own the named domain (verified via check_node_useable).
//   Parameters  :
//     name         [M]  Vec<u8>                       Plain label or dotted name.
//     record_type  [M]  RecordType (u16-backed enum)  DNS record type to write.
//                                                      SS58 (65280) and ORIGIN (65290)
//                                                      are BLOCKED — chain-managed only.
//     content      [M]  BoundedVec<u8, MaxContentLen> Raw encoded record content.
//                                                      MaxContentLen is a runtime constant.
//   READ  : nft::Tokens (ownership check via check_node_useable),
//           registry::RuntimeOrigin (parent expiry check via check_node_useable),
//           registrar::RegistrarInfos (expiry check via check_node_useable)
//   WRITE : resolvers::Records
//   NOTES : Fails with Ss58RecordProtected if record_type is SS58 or ORIGIN.
//           Fails with InvalidPermission if caller does not own the domain,
//           or if the domain (or its parent, for subdomains) is expired.
//           Name can be "domain" (top-level) or "sub.domain" (subdomain).
//
// [12.3] set_text
//   Description : Write a text record (email, Twitter handle, etc.) for a domain.
//   Caller      : Must own the named domain (verified via check_node_useable).
//   Parameters  :
//     name     [M]  Vec<u8>                       Plain label or dotted name.
//     kind     [M]  TextKind (enum)                One of: Email, Url, Avatar,
//                                                  Description, Notice, Keywords,
//                                                  Twitter, Github, Ipfs.
//     content  [M]  BoundedVec<u8, MaxContentLen> UTF-8 text value.
//   READ  : nft::Tokens, registry::RuntimeOrigin, registrar::RegistrarInfos
//   WRITE : resolvers::Texts
//   NOTES : Same permission model as set_record.
//           No restriction on TextKind — all variants are user-settable.
//
// ----------------------------------------------------------------------------
// PALLET: PnsMarketplace (index 13)
// ----------------------------------------------------------------------------
//
// [13.0] create_listing
//   Description : List the caller's canonical name for sale.
//   Caller      : Must own a canonical name (OwnerToPrimaryName must be set).
//   Parameters  :
//     price       [M]  Balance   Asking price in the native token (smallest unit).
//     expires_at  [M]  Moment    Unix millisecond timestamp for listing expiry.
//                                Must be strictly in the future.
//   READ  : registrar::OwnerToPrimaryName, marketplace::Listings
//   WRITE : marketplace::Listings
//   NOTES : Fails with AlreadyListed if the name is already listed.
//           Fails with ExpiryNotInFuture if expires_at <= now.
//           No escrow — name remains usable by owner while listed.
//           If owner transfers/releases the name, listing becomes stale.
//
// [13.1] cancel_listing
//   Description : Cancel the caller's active listing.
//   Caller      : Must own a canonical name with an active listing.
//   Parameters  : (none — listing identified by OwnerToPrimaryName[caller])
//   READ  : registrar::OwnerToPrimaryName, marketplace::Listings
//   WRITE : marketplace::Listings (removes entry)
//   NOTES : Fails with NoCanonicalName or NotListed.
//
// [13.2] buy_name
//   Description : Purchase a listed name at the asking price.
//                 Pass recipient=Some(account) to buy it as a gift.
//   Caller      : Any signed account (buyer != seller).
//   Parameters  :
//     name       [M]  Vec<u8>             Plain label of the name to buy (e.g. b"alice").
//     recipient  [O]  Option<AccountId>   If None → buy for self.
//                                         If Some(account) → gift purchase path:
//                                           • recipient must not already own any name
//                                           • name is placed in OfferedNames (lookups → null)
//                                           • recipient calls accept_offered_name to activate
//                                           • buyer ≠ recipient; seller ≠ recipient
//   READ  : marketplace::Listings, nft::Tokens,
//           registrar::OwnerToPrimaryName, registrar::RegistrarInfos,
//           registry::AccountToSubnames,
//           registrar::OfferedNames (gift path — checks for existing offer)
//   WRITE (standard path — recipient=None):
//           marketplace::Listings (removed),
//           registrar::OwnerToPrimaryName (seller removed; buyer inserted),
//           nft::Tokens, nft::TokensByOwner,
//           registry::SubNames, registry::SubnameRecords,
//           registry::OfferedToAccount, registry::AccountToSubnames,
//           registry::RuntimeOrigin,
//           resolvers::Records (SS58 + ORIGIN updated; other records cleared),
//           resolvers::Texts (cleared)
//   WRITE (gift path — recipient=Some):
//           marketplace::Listings (removed),
//           registrar::OfferedNames (inserted: node → { buyer, recipient }),
//           registrar::OwnerToPrimaryName (seller's entry removed),
//           nft::Tokens, nft::TokensByOwner (NFT transferred seller → recipient),
//           registry::SubNames, registry::SubnameRecords,
//           registry::OfferedToAccount, registry::AccountToSubnames,
//           registry::RuntimeOrigin,
//           resolvers::Records (SS58 + ORIGIN written for recipient; other records cleared),
//           resolvers::Texts (cleared)
//   NOTES : Protocol fee (ProtocolFeeBps / 10_000 × price) is burned from seller proceeds.
//           Buyer pays full `price`; seller receives `price − fee`.
//           Fails with BuyerIsSeller, BuyerIsRecipient, SellerIsRecipient,
//           ListingExpired, SellerNoLongerOwns, NameAlreadyOffered,
//           AlreadyHasCanonicalName, AlreadyHoldsSubdomain as appropriate.
//           Note: SS58 + ORIGIN are written at buy time even on gift path; they will
//           be overwritten again when recipient calls accept_offered_name.
//
// ============================================================================
// SECTION 2 — JSON-RPC METHODS (pns_ namespace, node/src/pns_rpc.rs)
// ============================================================================
//
// All methods are read-only — they never write to storage.
// They execute against the best (latest finalised) block.
// Each call passes through the PnsStorageApi runtime API.
//
// NOTE: Names in registrar::OfferedNames return null from pns_getInfo and
//       pns_resolveName — offered names are invisible to lookups until accepted.
//
// ----------------------------------------------------------------------------
//
// pns_getInfo
//   Method name : pns_getInfo
//   Description : Return the full name record for a domain by namehash.
//   Parameters  :
//     node  [M]  H256 (DomainHash)   Keccak256 namehash of the domain.
//   Returns     : NameRecord{ owner, expire, capacity, register_fee, for_sale,
//                             last_block, read_block_number, read_block_hash }
//                 or null if not found, expired, or in offered state.
//   READ  : registrar::OfferedNames (checks for offered state first),
//           registrar::RegistrarInfos,
//           nft::Tokens,
//           marketplace::Listings (for for_sale flag)
//   WRITE : (none)
//   SECURITY NOTES :
//     • Returns null during grace period (expire ≤ now < expire + GracePeriod) —
//       CAUTION: grace period logic differs between get_info versions; current
//       runtime uses pallet_timestamp::now() >= info.expire (strict expiry only).
//     • `read_block_number` and `read_block_hash` are populated by the RPC layer
//       from the best block header; they are NOT stored on chain.
//     • Accepts any 32-byte value as node; invalid hashes return null safely.
//
// pns_resolveName
//   Method name : pns_resolveName
//   Description : Resolve a human-readable label to its name record.
//   Parameters  :
//     name  [M]  String   Plain label ("alice") — top-level name only.
//                          Max 127 bytes validated at RPC layer.
//   Returns     : NameRecord or null.
//   READ  : registrar::OfferedNames,
//           registrar::RegistrarInfos,
//           nft::Tokens,
//           marketplace::Listings
//   WRITE : (none)
//   SECURITY NOTES :
//     • Name is converted to bytes, lowercased, and keccak256-hashed internally.
//     • Max 127-byte input guard at the RPC handler layer (validate_name_len).
//     • Only accepts single-label names (no dot parsing at RPC layer; the runtime
//       API's Label::new_with_len rejects names containing dots).
//     • Offered names (OfferedNames storage) return null.
//
// pns_lookup
//   Method name : pns_lookup
//   Description : Return all DNS records stored for a namehash.
//   Parameters  :
//     node  [M]  H256 (DomainHash)   Namehash of the domain.
//   Returns     : Array of (record_type_code: u32, content: bytes).
//                 record_type_code is the IANA type number (e.g. 65280 for SS58).
//   READ  : resolvers::Records (full prefix scan on node)
//   WRITE : (none)
//   SECURITY NOTES :
//     • Does NOT check name expiry or offered state — returns raw record data for
//       any hash, including expired or offered names.
//     • Includes SS58 (65280) and ORIGIN (65290) records in output.
//     • No upper bound on the number of records returned (full prefix scan).
//
// pns_getListing
//   Method name : pns_getListing
//   Description : Return the active marketplace listing for a plain label.
//   Parameters  :
//     name  [M]  String   Plain label (e.g. "alice"). Max 127 bytes.
//   Returns     : ListingInfo{ seller, price, expires_at,
//                              read_block_number, read_block_hash }
//                 or null if not listed.
//   READ  : marketplace::Listings
//   WRITE : (none)
//   SECURITY NOTES :
//     • Does NOT check whether listing is expired (expires_at check is the caller's
//       responsibility and is enforced only at buy_name execution time).
//     • read_block_number / read_block_hash populated by RPC layer, not chain.
//
// pns_nameToHash
//   Method name : pns_nameToHash
//   Description : Compute the namehash for a plain label or "sub.domain" string.
//   Parameters  :
//     name  [M]  String   Plain label ("alice") or dotted name ("sub.alice").
//                          Max 127 bytes validated at RPC layer.
//   Returns     : H256 namehash, or null if name is malformed.
//   READ  : (none — pure computation, no storage access)
//   WRITE : (none)
//   SECURITY NOTES :
//     • Safe to call with arbitrary input; returns null on validation failure.
//     • Lowercases input before hashing (case-insensitive).
//
// pns_lookupByName
//   Method name : pns_lookupByName
//   Description : Compute namehash then return all DNS records in one round-trip.
//   Parameters  :
//     name  [M]  String   Plain label or dotted name. Max 127 bytes.
//   Returns     : Array of (record_type_code: u32, content: bytes), or [] on invalid name.
//   READ  : resolvers::Records (full prefix scan)
//   WRITE : (none)
//   SECURITY NOTES :
//     • Same storage access and lack of expiry check as pns_lookup.
//     • Returns empty array (not null) on malformed name.
//
// pns_all
//   Method name : pns_all
//   Description : Return every registered name and its RegistrarInfo. Indexer use.
//   Parameters  : (none)
//   Returns     : Array of (DomainHash, RegistrarInfo{ expire, capacity,
//                            register_fee, label_len, last_block }).
//   READ  : registrar::RegistrarInfos (FULL TABLE SCAN — unbounded)
//   WRITE : (none)
//   SECURITY NOTES :
//     • UNBOUNDED SCAN. On a large chain this is a heavy call; should only be
//       used by indexers / explorers, not by wallets or latency-sensitive clients.
//     • Includes expired names. Does not filter by offered state.
//
// pns_isUseable
//   Method name : pns_isUseable
//   Description : Check whether a namehash is registered and not expired.
//   Parameters  :
//     node  [M]  H256 (DomainHash)
//   Returns     : bool — true if check_expires_useable passes.
//   READ  : registrar::RegistrarInfos
//   WRITE : (none)
//   SECURITY NOTES :
//     • Passes AccountId::zero() as the owner argument; the current runtime
//       implementation ignores owner and only checks RegistrarInfos expiry.
//     • Does NOT check offered state — an offered name may return true here
//       while pns_getInfo / pns_resolveName return null for the same name.
//       Clients should not rely on this method alone to determine name availability.
//     • Does not check grace period — uses check_expires_useable (strict: now < expire).
//
// ============================================================================
// SECTION 3 — RUNTIME API METHODS (PnsStorageApi, pns-runtime-api/src/lib.rs)
// ============================================================================
//
// These are callable via state_call on any Substrate node in addition to being
// the backing implementation for every pns_* JSON-RPC method above.
// All methods are read-only.
//
// PnsStorageApi::get_info(id: H256) → NameRecord | null
//   Same as pns_getInfo. See above.
//
// PnsStorageApi::all() → Vec<(H256, RegistrarInfo)>
//   Same as pns_all. See above.
//
// PnsStorageApi::lookup(id: H256) → Vec<(RecordType, bytes)>
//   Same as pns_lookup. See above.
//
// PnsStorageApi::check_node_useable(node: H256, owner: AccountId) → bool
//   Description : Returns true if the name exists, is not expired, and is owned by `owner`.
//   Parameters  :
//     node   [M]  H256       Namehash.
//     owner  [M]  AccountId  Account to verify ownership against.
//   READ  : registry::RuntimeOrigin (for subdomain parent expiry cascade),
//           registrar::RegistrarInfos (expiry check),
//           nft::Tokens (ownership check)
//   SECURITY NOTES :
//     • Subdomains derive expiry from their parent (cascaded via RuntimeOrigin).
//     • Returns false for names in the grace period (uses check_expires_useable,
//       not check_expires_renewable).
//
// PnsStorageApi::resolve_name(name: bytes) → NameRecord | null
//   Same as pns_resolveName. See above.
//
// PnsStorageApi::get_listing(name: bytes) → ListingInfo | null
//   Same as pns_getListing. See above.
//
// PnsStorageApi::name_to_hash(name: bytes) → H256 | null
//   Same as pns_nameToHash. See above.
//
// PnsStorageApi::lookup_by_name(name: bytes) → Vec<(RecordType, bytes)>
//   Same as pns_lookupByName. See above.
//
// PnsStorageApi::primary_name(owner: AccountId) → H256 | null
//   Description : Reverse lookup — return the canonical name hash for an account.
//   Parameters  :
//     owner  [M]  AccountId   Account to query.
//   Returns     : DomainHash, or null if no canonical name is registered.
//   READ  : registrar::OwnerToPrimaryName
//   SECURITY NOTES :
//     • Does NOT check expiry. An expired name still appears here until someone
//       registers over it (which removes the stale entry). Clients should
//       cross-check with get_info to verify liveness.
//     • Does NOT check offered state. A name in OfferedNames still has NO entry
//       in OwnerToPrimaryName (that entry is only set on accept_offered_name),
//       so this returns null for offered names — consistent with get_info.
//
// PnsStorageApi::subnames_of(owner: AccountId) → Vec<H256>
//   Description : Return all subname hashes currently held by an account.
//   Parameters  :
//     owner  [M]  AccountId
//   Returns     : Vec<DomainHash> (empty if none).
//   READ  : registry::AccountToSubnames (prefix scan on owner)
//   SECURITY NOTES :
//     • Returns Active subnames only (Offered/Rejected subnames are indexed in
//       OfferedToAccount, NOT AccountToSubnames).
//     • No expiry check — caller should verify parent liveness separately.
//     • Prefix scan; bounded by number of subnames per account.
//
// PnsStorageApi::get_subname(node: H256) → SubnameRecord | null
//   Description : Return the SubnameRecord for a subdomain hash.
//   Parameters  :
//     node  [M]  H256   Namehash of the subdomain.
//   Returns     : SubnameRecord{ parent: H256, label: bytes,
//                                target: AccountId, state: Offered|Active|Rejected }
//                 or null.
//   READ  : registry::SubnameRecords
//   SECURITY NOTES :
//     • Returns records in ALL states (Offered, Active, Rejected).
//     • Does NOT check parent expiry. A SubnameRecord may exist for a subdomain
//       whose parent has expired; always validate parent liveness via get_info.
//     • The `label` field is the raw label bytes (e.g. b"bob"), max 63 bytes.
//
// PnsStorageApi::pending_offers_for(account: AccountId) → Vec<H256>
//   Description : Return all subname hashes for which `account` has a pending
//                 subdomain offer (state = Offered in SubnameRecords).
//   Parameters  :
//     account  [M]  AccountId
//   Returns     : Vec<DomainHash>
//   READ  : registry::OfferedToAccount (prefix scan on account)
//   SECURITY NOTES :
//     • Only returns subnames in Offered state (Rejected ones are removed from
//       OfferedToAccount at rejection time).
//     • Does NOT include top-level name gift offers (use pending_name_offers_for).
//
// PnsStorageApi::pending_name_offers_for(account: AccountId) → Vec<H256>
//   Description : Return all TOP-LEVEL name hashes purchased as gifts for `account`
//                 that are awaiting acceptance in registrar::OfferedNames.
//   Parameters  :
//     account  [M]  AccountId
//   Returns     : Vec<DomainHash>
//   READ  : registrar::OfferedNames (FULL TABLE SCAN filtered by recipient)
//   SECURITY NOTES :
//     • UNBOUNDED FULL SCAN on OfferedNames — cost grows with total pending gifts
//       chain-wide. Acceptable at low volume; monitor under load.
//     • Does NOT check RegistrarInfo expiry. A gift could sit in OfferedNames after
//       the name's registration has expired. accept_offered_name will reject it with
//       NotUseable, but pending_name_offers_for will still list it here.
//     • Does NOT include subdomain offers (use pending_offers_for for those).
//
// ============================================================================
// SECTION 4 — CROSS-CUTTING SECURITY OBSERVATIONS
// ============================================================================
//
// [OBS-1] OFFERED NAME HALF-ACTIVATED STATE
//   When buy_name(gift path) executes, the NFT is transferred seller → recipient
//   and SS58/ORIGIN records are written for recipient, BUT OwnerToPrimaryName is
//   NOT set. This means:
//     • pns_getInfo / pns_resolveName return null (OfferedNames guard) ✓
//     • pns_isUseable may return true (no OfferedNames guard) ⚠
//     • check_node_useable returns false (OwnerToPrimaryName not set; NFT owner
//       doesn't match — wait, check_node_useable checks NFT owner vs passed owner)
//     • primary_name(recipient) returns null ✓
//     • The SS58 record in resolvers::Records contains recipient's address even
//       while the name is "null" to lookups — consistent after accept.
//
// [OBS-2] UNBOUNDED SCANS
//   pns_all and pending_name_offers_for perform full table scans.
//   Operators should rate-limit or disable pns_all on public nodes.
//
// [OBS-3] NO EXPIRY CHECK ON pns_lookup / pns_lookupByName
//   Raw record lookups return data regardless of name expiry or offered state.
//   Clients that use pns_lookup must independently verify liveness via pns_getInfo.
//
// [OBS-4] GRACE PERIOD INCONSISTENCY (resolved in current runtime)
//   PnsStorageApi::get_info in the current runtime uses a simple
//   `now >= info.expire` check (strict expiry, no grace window shown to clients).
//   The pns_rpc.rs wrapper previously used check_expires_renewable (which included
//   grace). Verify this is intentional for your deployment — grace period names
//   should only be renewable by the previous owner, not visible as active.
//
// [OBS-5] MISSING OFFERED STATE CHECK IN pns_isUseable
//   pns_isUseable returns true for names in OfferedNames (gift-pending state).
//   This is inconsistent with pns_getInfo / pns_resolveName which return null.
//   Clients MUST NOT use pns_isUseable to gate registration or availability checks.
//
// [OBS-6] PENDING_NAME_OFFERS_FOR FULL SCAN
//   registrar::OfferedNames is scanned linearly. If many gift purchases accumulate
//   (e.g. due to recipients never accepting), this call becomes expensive.
//   Consider adding an AccountId-indexed reverse storage in a future version.
//
// [OBS-7] GIFT REJECTION IS NON-REFUNDABLE
//   When a recipient rejects a gifted top-level name (via register's reject_offer),
//   the NFT is burned and the RegistrarInfos entry is cleared. The buyer's payment
//   is not refunded — consistent with the general "fees are burned" invariant.
//   This should be clearly communicated in any user-facing gift-purchase UI.
//
// [OBS-8] SUBDOMAIN EXPIRY CASCADES FROM PARENT
//   Subdomains have no independent expiry field. check_node_useable cascades
//   through RuntimeOrigin to check the root domain's RegistrarInfos expiry.
//   If a root domain expires, all subdomains become unusable simultaneously.
//   SubnameRecords are NOT auto-deleted on parent expiry — they linger until
//   the parent is transferred, released, or sold (which calls clear_subnames).
//
// ============================================================================
