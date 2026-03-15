/*!
# Resolvers
This module provides functionality for domain name resolution. Most of these interfaces are interfaces provided for subsequent cooperation with wallets.

### Module functions
- `set_account` - sets the account resolve, which requires the domain to be available relative to that user (ownership of the domain, the domain is not expired)
- `set_text` - set text parsing, same requirements as above
!*/

use codec::{Encode, MaxEncodedLen, DecodeWithMemTracking};

pub use pallet::*;

#[polkadot_sdk::frame_support::pallet]
pub mod pallet {
    use super::*;
    use codec::EncodeLike;
    use polkadot_sdk::frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use polkadot_sdk::frame_system::pallet_prelude::*;
    use pns_types::ddns::codec_type::RecordType;
    use scale_info::TypeInfo;
    use polkadot_sdk::sp_runtime::traits::AtLeast32BitUnsigned;

    use super::RegistryChecker;

    #[pallet::config]
    pub trait Config: polkadot_sdk::frame_system::Config {
        const OFFCHAIN_PREFIX: &'static [u8];

        type WeightInfo: WeightInfo;

        type MaxContentLen: Get<u32>;

        type AccountIndex: Parameter + Member + AtLeast32BitUnsigned + Default + Copy;

        type RegistryChecker: RegistryChecker<AccountId = Self::AccountId>;

        type Public: TypeInfo
            + Decode
            + Encode
            + EncodeLike
            + MaybeSerializeDeserialize
            + core::fmt::Debug
            + polkadot_sdk::sp_runtime::traits::IdentifyAccount<AccountId = Self::AccountId>;

        type Signature: polkadot_sdk::sp_runtime::traits::Verify<Signer = Self::Public>
            + codec::Codec
            + EncodeLike
            + MaybeSerializeDeserialize
            + Clone
            + Eq
            + core::fmt::Debug
            + TypeInfo;
    }

    pub type Content<T> = BoundedVec<u8, <T as Config>::MaxContentLen>;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[derive(Encode, Decode, Clone, Eq, PartialEq, MaxEncodedLen, Debug, TypeInfo, DecodeWithMemTracking)]
    #[derive(serde::Serialize, serde::Deserialize)]
    pub enum Address<Id> {
        Substrate([u8; 32]),
        Bitcoin([u8; 25]),
        Ethereum([u8; 20]),
        Id(Id),
    }

    /// account_id mapping
    #[pallet::storage]
    pub type Accounts<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        pns_types::DomainHash,
        Twox64Concat,
        Address<T::AccountId>,
        (),
    >;

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
        pub accounts: Vec<(pns_types::DomainHash, Address<T::AccountId>)>,
        pub texts: Vec<(pns_types::DomainHash, TextKind, Content<T>)>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            GenesisConfig {
                accounts: Vec::new(),
                texts: Vec::new(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for (node, address_kind) in self.accounts.iter().cloned() {
                Accounts::<T>::insert(node, address_kind, ());
            }
            for (node, text_kind, text) in self.texts.iter().cloned() {
                Texts::<T>::insert(node, text_kind, text);
            }
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AddressChanged {
            node: pns_types::DomainHash,
            address: Address<T::AccountId>,
        },
        AddressRemoved {
            node: pns_types::DomainHash,
            address: Address<T::AccountId>,
        },
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
        ParseAddressFailed,
        InvalidPermission,
        NotSupportedIndex,
        /// Address mapping does not exist for this node.
        AddressNotFound,
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
        /// SS58 is always included in the response. Any additional `record_types`
        /// requested are fetched individually — no other records are returned.
        /// Pass an empty vec to get only the SS58 address.
        pub fn lookup(node: pns_types::DomainHash, record_types: Vec<RecordType>) -> Vec<(RecordType, Vec<u8>)> {
            let mut results = Vec::new();
            let ss58 = Records::<T>::get(node, RecordType::SS58);
            if !ss58.is_empty() {
                results.push((RecordType::SS58, ss58.to_vec()));
            }
            for rt in record_types {
                if rt != RecordType::SS58 {
                    let content = Records::<T>::get(node, rt);
                    if !content.is_empty() {
                        results.push((rt, content.to_vec()));
                    }
                }
            }
            results
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Set an address mapping for a domain.
        ///
        /// `name` accepts two forms:
        /// - `"sub.domain"` — targets the subdomain `sub.domain.<tld>`.
        /// - `"domain"` (no dot) — targets the top-level domain `domain.<tld>`.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_account())]
        pub fn set_account(
            origin: OriginFor<T>,
            name: Vec<u8>,
            address: Address<T::AccountId>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let node = Self::name_to_node(&name)?;
            ensure!(
                T::RegistryChecker::check_node_useable(node, &who),
                Error::<T>::InvalidPermission
            );
            Accounts::<T>::insert(node, &address, ());
            Self::deposit_event(Event::<T>::AddressChanged { node, address });
            Ok(())
        }

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
            // SS58 is managed by the chain (register / transfer / buy); owners cannot
            // overwrite it via this extrinsic.
            ensure!(
                record_type != RecordType::SS58,
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

        /// Remove a specific address mapping for a domain.
        ///
        /// `name` accepts two forms:
        /// - `"sub.domain"` — targets the subdomain `sub.domain.<tld>`.
        /// - `"domain"` (no dot) — targets the top-level domain `domain.<tld>`.
        ///
        /// The caller must own (or be an approved operator of) the domain and
        /// the domain must not be expired.  Unlike `set_text`/`set_record`,
        /// address entries in `Accounts` have no default value, so a dedicated
        /// extrinsic is required to delete them.
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::remove_account())]
        pub fn remove_account(
            origin: OriginFor<T>,
            name: Vec<u8>,
            address: Address<T::AccountId>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let node = Self::name_to_node(&name)?;
            ensure!(
                T::RegistryChecker::check_node_useable(node, &who),
                Error::<T>::InvalidPermission
            );
            ensure!(
                Accounts::<T>::contains_key(node, &address),
                Error::<T>::AddressNotFound
            );
            Accounts::<T>::remove(node, &address);
            Self::deposit_event(Event::<T>::AddressRemoved { node, address });
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
    fn set_account() -> Weight;
    fn remove_account() -> Weight;
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
    fn set_account() -> Weight { Weight::zero() }
    fn remove_account() -> Weight { Weight::zero() }
}

impl<C: Config> Pallet<C> {
    pub fn lookup_all(id: DomainHash) -> Vec<(RecordType, Vec<u8>)> {
        Records::<C>::iter_prefix(id)
            .map(|(k2, v)| (k2, v.to_vec()))
            .collect::<Vec<(RecordType, Vec<u8>)>>()
    }

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
            .map_err(|_| pallet::Error::<C>::ParseAddressFailed)?;
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
            .map_err(|_| pallet::Error::<C>::ParseAddressFailed)?;
        pallet::Records::<C>::insert(node, RecordType::ORIGIN, content);
        Ok(())
    }

    /// Remove all DNS records for `node` except the SS58 record.
    ///
    /// Called on ownership transfers so the new owner does not inherit stale
    /// DNS data (RPC endpoints, validator stash, parachain IDs, etc.) from
    /// the previous owner.  `Accounts` and `Texts` are intentionally left
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

    /// Remove ALL DNS records for `node` (Records, Accounts, and Texts).
    ///
    /// Called when a domain is completely destroyed (e.g. a subname that is
    /// auto-cleared when its parent is transferred, released, or sold).
    pub fn clear_all_records(node: DomainHash) {
        let _ = pallet::Records::<C>::clear_prefix(node, u32::MAX, None);
        let _ = pallet::Accounts::<C>::clear_prefix(node, u32::MAX, None);
        let _ = pallet::Texts::<C>::clear_prefix(node, u32::MAX, None);
    }
}