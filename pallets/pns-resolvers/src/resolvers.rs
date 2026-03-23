/*!
# Resolvers
This module provides functionality for domain name resolution. Most of these interfaces are interfaces provided for subsequent cooperation with wallets.

### Module functions
- `set_text` - set text parsing, requires domain ownership and the domain to not be expired
- `set_record` - set a DNS record, same requirements as above
!*/

use codec::{Encode, MaxEncodedLen, DecodeWithMemTracking};

pub use pallet::*;

#[polkadot_sdk::frame_support::pallet]
pub mod pallet {
    use super::*;
    use polkadot_sdk::frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use polkadot_sdk::frame_system::pallet_prelude::*;
    use pns_types::ddns::codec_type::RecordType;
    use scale_info::TypeInfo;

    use super::RegistryChecker;

    #[pallet::config]
    pub trait Config: polkadot_sdk::frame_system::Config {
        const OFFCHAIN_PREFIX: &'static [u8];

        type WeightInfo: WeightInfo;

        type MaxContentLen: Get<u32>;

        type RegistryChecker: RegistryChecker<AccountId = Self::AccountId>;
    }

    pub type Content<T> = BoundedVec<u8, <T as Config>::MaxContentLen>;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[derive(Encode, Decode, Clone, Eq, PartialEq, MaxEncodedLen, Debug, TypeInfo, DecodeWithMemTracking)]
    #[derive(serde::Serialize, serde::Deserialize)]
    pub enum TextKind {
        Email,
        Url,
        Avatar,
        Description,
        Notice,
        Keywords,
        Twitter,
        Github,
        Ipfs,
    }

    /// text mapping
    #[pallet::storage]
    pub type Texts<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        pns_types::DomainHash,
        Twox64Concat,
        TextKind,
        Content<T>,
        ValueQuery,
    >;

    /// ddns record
    #[pallet::storage]
    pub type Records<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        pns_types::DomainHash,
        Twox64Concat,
        pns_types::ddns::codec_type::RecordType,
        Content<T>,
        ValueQuery,
    >;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub texts: Vec<(pns_types::DomainHash, TextKind, Content<T>)>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            GenesisConfig {
                texts: Vec::new(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for (node, text_kind, text) in self.texts.iter().cloned() {
                Texts::<T>::insert(node, text_kind, text);
            }
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        TextsChanged {
            node: pns_types::DomainHash,
            kind: TextKind,
            content: Content<T>,
        },
        RecordsChanged {
            node: pns_types::DomainHash,
            kind: RecordType,
            content: Content<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidPermission,
        /// The SS58 record is managed by the chain (set on register/transfer/buy).
        /// Owners may not edit it directly via `set_record`.
        Ss58RecordProtected,
        /// The name string you provided is invalid (illegal characters, wrong length,
        /// or unsupported format).  Use `"sub.domain"` for subdomains or `"domain"`
        /// for top-level names.
        InvalidName,
    }

    impl<T: Config> Pallet<T> {
        /// Resolve a human-readable name string to a [`DomainHash`].
        ///
        /// - `"sub.domain"` → namehash of subdomain `sub` under `domain.<tld>`.
        /// - `"domain"` (no dot) → namehash of the top-level `domain.<tld>`.
        fn name_to_node(name: &[u8]) -> Result<pns_types::DomainHash, polkadot_sdk::sp_runtime::DispatchError> {
            pns_types::parse_name_to_node(name, &T::RegistryChecker::base_node())
                .ok_or(Error::<T>::InvalidName.into())
        }
    }

    #[pallet::view_functions]
    impl<T: Config> Pallet<T> {
        /// Return DNS records for a domain namehash.
        ///
        /// SS58 (65280) and ORIGIN (65290) are always included in the response as
        /// proof anchors — they are chain-managed and not caller-controlled.
        /// Additional types are fetched only if explicitly listed in `record_types`.
        ///
        /// The following types are blocked from `record_types` and silently dropped
        /// if present — they are either auto-included, protocol-level DNS mechanism
        /// records that are never stored per-name, or bulk transfer types:
        ///   SS58, ORIGIN, ANY, AXFR, IXFR, OPT, TSIG, SIG, RRSIG,
        ///   NSEC, NSEC3, NSEC3PARAM, DNSKEY, CDNSKEY, CDS, DS
        pub fn lookup(node: pns_types::DomainHash, record_types: Vec<RecordType>) -> Vec<(RecordType, Vec<u8>)> {
            const BLOCKED: &[RecordType] = &[
                RecordType::SS58,       // chain-managed, always returned unconditionally
                RecordType::ORIGIN,     // chain-managed, always returned unconditionally
                RecordType::ANY,        // bulk request — RFC 8482 deprecated
                RecordType::AXFR,       // zone transfer
                RecordType::IXFR,       // incremental zone transfer
                RecordType::OPT,        // EDNS meta-record, not zone data
                RecordType::TSIG,       // DNS transaction signature
                RecordType::SIG,        // obsolete DNSSEC signature
                RecordType::RRSIG,      // DNSSEC resource record signature
                RecordType::NSEC,       // DNSSEC denial-of-existence
                RecordType::NSEC3,      // DNSSEC denial-of-existence v3
                RecordType::NSEC3PARAM, // DNSSEC NSEC3 parameters
                RecordType::DNSKEY,     // DNSSEC zone signing key
                RecordType::CDNSKEY,    // child DNSKEY
                RecordType::CDS,        // child DS
                RecordType::DS,         // delegation signer
            ];

            let mut results = Vec::new();

            // SS58 and ORIGIN are proof anchors — included unconditionally.
            let ss58 = Records::<T>::get(node, RecordType::SS58);
            if !ss58.is_empty() {
                results.push((RecordType::SS58, ss58.to_vec()));
            }
            let origin = Records::<T>::get(node, RecordType::ORIGIN);
            if !origin.is_empty() {
                results.push((RecordType::ORIGIN, origin.to_vec()));
            }

            for rt in record_types {
                if BLOCKED.contains(&rt) {
                    continue;
                }
                let content = Records::<T>::get(node, rt);
                if !content.is_empty() {
                    results.push((rt, content.to_vec()));
                }
            }
            results
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Set a DNS record for a domain.
        ///
        /// `name` accepts two forms:
        /// - `"sub.domain"` — targets the subdomain `sub.domain.<tld>`.
        /// - `"domain"` (no dot) — targets the top-level domain `domain.<tld>`.
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_record(content.len() as u32))]
        pub fn set_record(
            origin: OriginFor<T>,
            name: Vec<u8>,
            record_type: RecordType,
            content: Content<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // SS58 and ORIGIN are managed by the chain and serve as trust anchors;
            // owners cannot overwrite them via this extrinsic.
            ensure!(
                record_type != RecordType::SS58 && record_type != RecordType::ORIGIN,
                Error::<T>::Ss58RecordProtected
            );
            let node = Self::name_to_node(&name)?;
            ensure!(
                T::RegistryChecker::check_node_useable(node, &who),
                Error::<T>::InvalidPermission
            );
            Records::<T>::insert(node, &record_type, &content);
            Self::deposit_event(Event::<T>::RecordsChanged {
                node,
                kind: record_type,
                content,
            });
            Ok(())
        }

        /// Set a text record for a domain.
        ///
        /// `name` accepts two forms:
        /// - `"sub.domain"` — targets the subdomain `sub.domain.<tld>`.
        /// - `"domain"` (no dot) — targets the top-level domain `domain.<tld>`.
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_text(content.len() as u32))]
        pub fn set_text(
            origin: OriginFor<T>,
            name: Vec<u8>,
            kind: TextKind,
            content: Content<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let node = Self::name_to_node(&name)?;
            ensure!(
                T::RegistryChecker::check_node_useable(node, &who),
                Error::<T>::InvalidPermission
            );
            Texts::<T>::insert(node, &kind, &content);
            Self::deposit_event(Event::<T>::TextsChanged { node, kind, content });
            Ok(())
        }

    }
}

use polkadot_sdk::frame_support::dispatch::DispatchResult;
use polkadot_sdk::sp_weights::Weight;
use pns_types::{ddns::codec_type::RecordType, DomainHash};
use polkadot_sdk::sp_std::vec::Vec;

pub trait WeightInfo {
    fn set_text(content_len: u32) -> Weight;
    fn set_record(content_len: u32) -> Weight;
}

pub trait RegistryChecker {
    type AccountId;
    fn check_node_useable(node: pns_types::DomainHash, owner: &Self::AccountId) -> bool;
    /// Returns the TLD base node used by the chain (e.g. `DOT_BASENODE`).
    /// Required so the resolver pallet can compute domain hashes from name strings.
    fn base_node() -> pns_types::DomainHash;
}

impl WeightInfo for () {
    fn set_text(_content_len: u32) -> Weight { Weight::zero() }
    fn set_record(_content_len: u32) -> Weight { Weight::zero() }
}

impl<C: Config> Pallet<C> {
    /// Write the SS58 record for `node` from the owner's encoded account bytes.
    ///
    /// This is an internal privileged path: it bypasses the normal user-facing
    /// permission check and is intended to be called only by trusted pallets
    /// (registrar on registration, registry on transfer, marketplace on buy).
    pub fn set_ss58_record(node: DomainHash, owner: &C::AccountId) -> DispatchResult
    where
        C::AccountId: codec::Encode,
    {
        use codec::Encode;
        let bytes = owner.encode();
        let content = pallet::Content::<C>::try_from(bytes)
            .map_err(|_| pallet::Error::<C>::InvalidPermission)?;
        pallet::Records::<C>::insert(node, RecordType::SS58, content);
        Ok(())
    }

    /// Write the ORIGIN record for `node` — the block hash of the block in which
    /// the name was originally registered. Used as on-chain proof of purchase.
    ///
    /// Like `set_ss58_record`, this is an internal privileged path called only
    /// by the registrar on registration.
    pub fn set_origin_record(node: DomainHash, block_hash: [u8; 32]) -> DispatchResult {
        let content = pallet::Content::<C>::try_from(block_hash.to_vec())
            .map_err(|_| pallet::Error::<C>::InvalidPermission)?;
        pallet::Records::<C>::insert(node, RecordType::ORIGIN, content);
        Ok(())
    }

    /// Remove all DNS records for `node` except the SS58 record.
    ///
    /// Called on ownership transfers so the new owner does not inherit stale
    /// DNS data (RPC endpoints, validator stash, parachain IDs, etc.) from
    /// the previous owner.  `Texts` are intentionally left
    /// untouched — only `Records` entries are cleared here.
    pub fn clear_records_except_ss58(node: DomainHash) {
        let to_remove: Vec<RecordType> = pallet::Records::<C>::iter_prefix(node)
            .filter(|(rt, _)| *rt != RecordType::SS58)
            .map(|(rt, _)| rt)
            .collect();
        for rt in to_remove {
            pallet::Records::<C>::remove(node, rt);
        }
    }

    /// Remove ALL DNS records for `node` (Records and Texts).
    ///
    /// Called when a domain is completely destroyed (e.g. a subname that is
    /// auto-cleared when its parent is transferred, released, or sold).
    pub fn clear_all_records(node: DomainHash) {
        let _ = pallet::Records::<C>::clear_prefix(node, u32::MAX, None);
        let _ = pallet::Texts::<C>::clear_prefix(node, u32::MAX, None);
    }
}