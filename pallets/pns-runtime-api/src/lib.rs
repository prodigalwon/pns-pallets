#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unnecessary_mut_passed)]

use pns_types::{ddns::codec_type::RecordType, DomainHash, ListingInfo, NameRecord, RegistrarInfo};
use polkadot_sdk::sp_runtime::traits::MaybeSerialize;
use codec::{Decode, Encode};

polkadot_sdk::sp_api::decl_runtime_apis! {
    pub trait PnsStorageApi<Duration, Balance, AccountId>
    where
        Duration: Decode + Encode + MaybeSerialize,
        Balance: Decode + Encode + MaybeSerialize,
        AccountId: Decode + Encode + MaybeSerialize,
    {
        fn get_info(id: DomainHash) -> Option<NameRecord<AccountId, Duration, Balance>>;
        fn all() -> polkadot_sdk::sp_std::vec::Vec<(DomainHash, RegistrarInfo<Duration, Balance>)>;
        fn lookup(id: DomainHash) -> polkadot_sdk::sp_std::vec::Vec<(RecordType, polkadot_sdk::sp_std::vec::Vec<u8>)>;
        fn check_node_useable(node: DomainHash, owner: &AccountId) -> bool;
        /// Resolve a plain label (e.g. b"alice") to the full name record including owner.
        /// Computes the namehash internally against the native base node.
        fn resolve_name(name: polkadot_sdk::sp_std::vec::Vec<u8>) -> Option<NameRecord<AccountId, Duration, Balance>>;
        /// Return the active marketplace listing for a plain label (e.g. b"alice"), or `None` if not listed.
        fn get_listing(name: polkadot_sdk::sp_std::vec::Vec<u8>) -> Option<ListingInfo<AccountId, Balance, Duration>>;
        /// Compute the namehash for a plain label or dotted name (e.g. b"alice" or b"sub.alice").
        /// Returns `None` if the name contains invalid characters or is malformed.
        /// Use this to obtain the `DomainHash` required by hash-based calls like `get_info` and `lookup`.
        fn name_to_hash(name: polkadot_sdk::sp_std::vec::Vec<u8>) -> Option<DomainHash>;
        /// Return all DNS records for a plain label or dotted name (e.g. b"alice" or b"sub.alice").
        /// Equivalent to calling `name_to_hash` then `lookup`, but in a single round-trip.
        fn lookup_by_name(name: polkadot_sdk::sp_std::vec::Vec<u8>) -> polkadot_sdk::sp_std::vec::Vec<(RecordType, polkadot_sdk::sp_std::vec::Vec<u8>)>;
        /// Reverse lookup: return the namehash of the canonical (primary) name registered to
        /// `owner`, or `None` if the account has no canonical name.
        /// Use `get_info` on the returned hash to get the full `NameRecord`.
        fn primary_name(owner: AccountId) -> Option<DomainHash>;
        /// Return the namehashes of all subnames currently held by `owner`.
        /// Returns an empty vec if the account holds no subnames.
        fn subnames_of(owner: AccountId) -> polkadot_sdk::sp_std::vec::Vec<DomainHash>;
        /// Return the SubnameRecord for a subname hash, or None if it does not exist.
        /// The record includes the parent hash, label, target, and state (Offered/Active/Rejected).
        fn get_subname(node: DomainHash) -> Option<pns_types::SubnameRecord<AccountId>>;
        /// Return all subname hashes for which `account` has a pending offer (state = Offered).
        fn pending_offers_for(account: AccountId) -> polkadot_sdk::sp_std::vec::Vec<DomainHash>;
    }
}