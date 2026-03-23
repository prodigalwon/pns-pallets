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
        fn lookup(id: DomainHash, record_types: polkadot_sdk::sp_std::vec::Vec<RecordType>) -> polkadot_sdk::sp_std::vec::Vec<(RecordType, polkadot_sdk::sp_std::vec::Vec<u8>)>;
        /// Resolve a plain label (e.g. b"alice") to the full name record including owner.
        /// Computes the namehash internally against the native base node.
        fn resolve_name(name: polkadot_sdk::sp_std::vec::Vec<u8>) -> Option<NameRecord<AccountId, Duration, Balance>>;
        /// Return the active marketplace listing for a plain label (e.g. b"alice"), or `None` if not listed.
        fn get_listing(name: polkadot_sdk::sp_std::vec::Vec<u8>) -> Option<ListingInfo<AccountId, Balance, Duration>>;
        /// Return all DNS records for a plain label or dotted name (e.g. b"alice" or b"sub.alice").
        /// Equivalent to calling name_to_hash then `lookup`, but in a single round-trip.
        fn lookup_by_name(name: polkadot_sdk::sp_std::vec::Vec<u8>, record_types: polkadot_sdk::sp_std::vec::Vec<RecordType>) -> polkadot_sdk::sp_std::vec::Vec<(RecordType, polkadot_sdk::sp_std::vec::Vec<u8>)>;
        /// Return the SubnameRecord for a subname hash, or None if it does not exist.
        /// The record includes the parent hash, label, target, and state (Offered/Active/Rejected).
        fn get_subname(node: DomainHash) -> Option<pns_types::SubnameRecord<AccountId>>;
        /// Return a full name portfolio summary for an account in a single call.
        /// Includes primary name, active subnames, pending subdomain offers, and pending name gift offers.
        fn account_dashboard(owner: AccountId) -> pns_types::AccountDashboard;
    }
}