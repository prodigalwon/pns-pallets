//! # Registrar
//! This module is the registration center for domain names,
//! and it also records some important information about domain name registration:
//!
//! ```rust
//!     pub struct RegistrarInfo<Duration, Balance> {
//!         /// Expiration time
//!         pub expire: Duration,
//!         /// Capacity of subdomains that can be created
//!         pub capacity: u32,
//!         /// Registration fee (burned at registration time)
//!         pub register_fee: Balance,
//!     }
//! ```
//! ## Introduction
//! Some of the methods in this module involve the transfer of money,
//! so you need to be as careful as possible when reviewing them.
//!
//! Registration and renewal fees are burned via `Currency::withdraw` drop.
//! There is no deposit — the fee is non-refundable and covers the cost of
//! occupying chain state for the lifetime of the registration.
//!
//! ### Module functions
//! - `add_reserved` - adds a pre-reserved domain name (pre-reserved domains cannot be registered), requires manager privileges
//! - `remove_reserved` - removes a reserved domain name, requires manager privileges
//! - `register` - register a domain name
//! - `renew` - renew a domain name, requires caller to own the canonical name
//! - `transfer` - transfer a domain name, requires the caller to own the canonical name
//! - `release_name` - burn the caller's canonical name, returning it to the open pool
//! - `mint_subname` - create a subdomain, requires the caller to have permission to operate the domain

pub use pallet::*;
use pns_types::DomainHash;

pub type BalanceOf<T> = <<T as Config>::Currency as polkadot_sdk::frame_support::traits::Currency<
    <T as polkadot_sdk::frame_system::Config>::AccountId,
>>::Balance;

#[polkadot_sdk::frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::traits::{IsRegistrarOpen, Label, Official, OriginRecorder, PriceOracle, RecordCleaner, Registry, Ss58Updater};
    use polkadot_sdk::frame_support::{
        pallet_prelude::*,
        traits::{Currency, EnsureOrigin, ExistenceRequirement, Time},
        Twox64Concat,
    };
    use polkadot_sdk::frame_system::{ensure_signed, pallet_prelude::*};
    use pns_types::{DomainHash, RegistrarInfo};
    use polkadot_sdk::sp_runtime::traits::{AtLeast32Bit, CheckedAdd, MaybeSerializeDeserialize, StaticLookup};
    use polkadot_sdk::sp_runtime::{ArithmeticError, SaturatedConversion};
    use polkadot_sdk::sp_std::vec::Vec;

    #[pallet::config]
    pub trait Config: polkadot_sdk::frame_system::Config {

        type Registry: Registry<AccountId = Self::AccountId, Balance = BalanceOf<Self>>;

        type Currency: Currency<Self::AccountId>;

        type NowProvider: Time<Moment = Self::Moment>;

        type Moment: AtLeast32Bit
            + Parameter
            + Default
            + Copy
            + MaxEncodedLen
            + MaybeSerializeDeserialize;

        #[pallet::constant]
        type GracePeriod: Get<Self::Moment>;

        #[pallet::constant]
        type DefaultCapacity: Get<u32>;

        #[pallet::constant]
        type BaseNode: Get<DomainHash>;

        #[pallet::constant]
        type MinRegistrationDuration: Get<Self::Moment>;

        #[pallet::constant]
        type MaxRegistrationDuration: Get<Self::Moment>;

        type WeightInfo: WeightInfo;

        type PriceOracle: PriceOracle<Moment = Self::Moment, Balance = BalanceOf<Self>>;

        type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

        type IsOpen: IsRegistrarOpen;

        type Official: Official<AccountId = Self::AccountId>;

        /// Writes the SS58 record when a name is registered.
        type Ss58Updater: Ss58Updater<AccountId = Self::AccountId>;

        /// Writes the ORIGIN record (parent block hash) on initial registration.
        type OriginRecorder: OriginRecorder;

        /// Clears DNS records when a subdomain is revoked or released.
        type RecordCleaner: RecordCleaner;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// `name_hash` -> Info{ `expire`, `capacity`, `register_fee`, `label_len` }
    #[pallet::storage]
    pub type RegistrarInfos<T: Config> =
        StorageMap<_, Blake2_128Concat, DomainHash, RegistrarInfoOf<T>>;

    /// `name_hash` if in `reserved_list` -> ()
    #[pallet::storage]
    pub type ReservedList<T: Config> = StorageMap<_, Twox64Concat, DomainHash, (), ValueQuery>;

    /// `owner` -> their single canonical `name_hash`
    /// Each address may hold at most one canonical (top-level) name at a time.
    #[pallet::storage]
    pub type OwnerToPrimaryName<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, DomainHash>;

    pub type RegistrarInfoOf<T> = RegistrarInfo<<T as Config>::Moment, BalanceOf<T>>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub infos: Vec<(DomainHash, RegistrarInfoOf<T>)>,
        pub reserved_list: polkadot_sdk::sp_std::collections::btree_set::BTreeSet<DomainHash>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            GenesisConfig {
                infos: Vec::with_capacity(0),
                reserved_list: polkadot_sdk::sp_std::collections::btree_set::BTreeSet::new(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for (node, info) in self.infos.iter() {
                RegistrarInfos::<T>::insert(node, info);
            }

            for node in self.reserved_list.iter() {
                ReservedList::<T>::insert(node, ());
            }
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// When a domain name is successfully registered, this moment will be logged.
        NameRegistered {
            name: Vec<u8>,
            node: DomainHash,
            owner: T::AccountId,
            expire: T::Moment,
            fee: BalanceOf<T>,
        },
        // to frontend call
        /// When a domain name is successfully renewed, this moment will be logged.
        NameRenewed {
            node: DomainHash,
            owner: T::AccountId,
            expire: T::Moment,
            fee: BalanceOf<T>,
        },
        SubnameOffered {
            parent: DomainHash,
            subnode: DomainHash,
            label: Vec<u8>,
            target: T::AccountId,
        },
        SubnameAccepted {
            subnode: DomainHash,
            target: T::AccountId,
        },
        SubnameRejected {
            subnode: DomainHash,
            target: T::AccountId,
        },
        SubnameRevoked {
            subnode: DomainHash,
            parent: DomainHash,
        },
        SubnameReleased {
            subnode: DomainHash,
            parent: DomainHash,
        },
        /// Reserve a domain name.
        NameReserved { node: DomainHash },
        /// Cancel a reserved domain name.
        NameUnReserved { node: DomainHash },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// You are not in possession of the term.
        NotOwned,
        /// The node is still occupied and cannot be registered.
        Occupied,
        /// You are processing a subdomain or the domain which does not exist.
        /// Or you are registering an occupied subdomain.
        NotExistOrOccupied,
        /// This domain name is temporarily frozen, if you are the authority of the
        /// country (region) or organization, you can contact the official to get
        /// this domain name for you.
        Frozen,
        /// The label you entered is not parsed properly, maybe there are illegal characters in your label.
        ParseLabelFailed,
        /// The length of the label you entered does not correspond to the requirement.
        ///
        /// The length of the label is calculated according to bytes.
        LabelInvalid,
        /// The domain name has exceeded its trial period, please renew or re-register.
        NotUseable,
        /// The domain name has exceeded the renewal period, please re-register.
        NotRenewable,
        /// You want to register in less time than the minimum time we set.
        RegistryDurationInvalid,
        /// Sorry, the registration center is currently closed, please pay attention to the official message and wait for the registration to open.
        RegistrarClosed,
        /// This address already owns a canonical name. Release the current name before registering a new one.
        AlreadyHasCanonicalName,
        /// This address already holds an active subdomain. Release it before acquiring a canonical name.
        AlreadyHoldsSubdomain,
        /// This address has no canonical name registered.
        NoCanonicalName,
        /// Subdomain label could not be converted to a bounded vector (too long).
        LabelTooLong,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Add a domain name to the reserved list by name string (e.g. b"alice").
        ///
        /// Reserved names cannot be registered by anyone until removed.
        /// Only callable by the manager origin.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::add_reserved())]
        pub fn add_reserved(origin: OriginFor<T>, name: Vec<u8>) -> DispatchResult {
            let _who = T::ManagerOrigin::ensure_origin(origin)?;

            let (label, _) = Label::new_with_len(&name).ok_or(Error::<T>::ParseLabelFailed)?;
            let node = label.encode_with_node(&T::BaseNode::get());

            ReservedList::<T>::insert(node, ());

            Self::deposit_event(Event::<T>::NameReserved { node });
            Ok(())
        }
        /// Remove a domain name from the reserved list by name string (e.g. b"alice").
        ///
        /// Only callable by the manager origin.
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::remove_reserved())]
        pub fn remove_reserved(origin: OriginFor<T>, name: Vec<u8>) -> DispatchResult {
            let _who = T::ManagerOrigin::ensure_origin(origin)?;

            let (label, _) = Label::new_with_len(&name).ok_or(Error::<T>::ParseLabelFailed)?;
            let node = label.encode_with_node(&T::BaseNode::get());

            ReservedList::<T>::remove(node);

            Self::deposit_event(Event::<T>::NameUnReserved { node });
            Ok(())
        }
        /// Register a domain name.
        ///
        /// Note: The domain name must conform to the rules,
        /// while the interface is only responsible for
        /// registering domain names greater than 10 in length.
        ///
        /// Ensure: The name must be unoccupied.
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::register(name.len() as u32))]
        #[polkadot_sdk::frame_support::transactional]
        pub fn register(
            origin: OriginFor<T>,
            name: Vec<u8>,
            owner: <T::Lookup as StaticLookup>::Source,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let owner = T::Lookup::lookup(owner)?;

            ensure!(!name.is_empty(), Error::<T>::LabelInvalid);
            ensure!(T::IsOpen::is_open(), Error::<T>::RegistrarClosed);

            let (label, label_len) =
                Label::new_with_len(&name).ok_or(Error::<T>::ParseLabelFailed)?;

            use crate::traits::Available;

            ensure!(label_len.is_registrable(), Error::<T>::LabelInvalid);

            let official = T::Official::get_official_account()?;

            let now = T::NowProvider::now();
            let duration = T::MaxRegistrationDuration::get();
            let expire = now
                .checked_add(&duration)
                .ok_or(ArithmeticError::Overflow)?;

            let base_node = T::BaseNode::get();
            let label_node = label.encode_with_node(&base_node);

            ensure!(
                !ReservedList::<T>::contains_key(label_node),
                Error::<T>::Frozen
            );

            // Enforce one name per address: no canonical name and no active subdomain.
            if let Some(existing_node) = OwnerToPrimaryName::<T>::get(&owner) {
                let still_valid = RegistrarInfos::<T>::get(existing_node)
                    .map(|info| now <= info.expire + T::GracePeriod::get())
                    .unwrap_or(false);
                if still_valid {
                    return Err(Error::<T>::AlreadyHasCanonicalName.into());
                }
                // Expired canonical name – remove the stale entry so the owner can register anew.
                OwnerToPrimaryName::<T>::remove(&owner);
            }
            ensure!(!T::Registry::has_active_subname(&owner), Error::<T>::AlreadyHoldsSubdomain);

            let fee = T::PriceOracle::registration_fee(label_len)
                .ok_or(ArithmeticError::Overflow)?;

            T::Registry::mint_subname(
                &official,
                base_node,
                label_node,
                owner.clone(),
                0,
                |maybe_pre_owner| -> DispatchResult {
                    // If this name was previously owned by someone else, remove their
                    // canonical name record so they are free to register again.
                    if let Some(pre_owner) = maybe_pre_owner {
                        if OwnerToPrimaryName::<T>::get(pre_owner) == Some(label_node) {
                            OwnerToPrimaryName::<T>::remove(pre_owner);
                        }
                    }
                    let imbalance = T::Currency::withdraw(
                        &caller,
                        fee,
                        polkadot_sdk::frame_support::traits::WithdrawReasons::FEE,
                        ExistenceRequirement::KeepAlive,
                    )?;
                    drop(imbalance);
                    let current_block = polkadot_sdk::frame_system::Pallet::<T>::block_number().saturated_into::<u32>();
                    RegistrarInfos::<T>::mutate(label_node, |info| -> DispatchResult {
                        if let Some(info) = info.as_mut() {
                            info.register_fee = fee;
                            info.expire = expire;
                            info.label_len = label_len as u32;
                            info.last_block = current_block;
                        } else {
                            let _ = info.insert(RegistrarInfoOf::<T> {
                                register_fee: fee,
                                expire,
                                capacity: T::DefaultCapacity::get(),
                                label_len: label_len as u32,
                                last_block: current_block,
                            });
                        }
                        Ok(())
                    })?;
                    Ok(())
                },
            )?;

            // Record this as the owner's canonical name.
            OwnerToPrimaryName::<T>::insert(&owner, label_node);

            // Write the SS58 record so DNS lookups immediately resolve to the new owner.
            T::Ss58Updater::update_ss58(label_node, &owner)?;

            // Write the ORIGIN record — parent block hash as proof of registration block.
            let parent_hash: [u8; 32] = polkadot_sdk::frame_system::Pallet::<T>::parent_hash()
                .as_ref()
                .try_into()
                .unwrap_or([0u8; 32]);
            T::OriginRecorder::record_origin(label_node, parent_hash)?;

            Self::deposit_event(Event::<T>::NameRegistered {
                name,
                node: label_node,
                owner,
                expire,
                fee,
            });

            Ok(())
        }
        /// Renew the caller's canonical domain name.
        ///
        /// Only the current owner of a name may renew it. Renewal resets expiry to
        /// `MaxRegistrationDuration` from now (not additive) and charges the renewal
        /// fee to the caller.
        ///
        /// Ensure: Caller must own a canonical name that is within the renewable period.
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::renew())]
        #[polkadot_sdk::frame_support::transactional]
        pub fn renew(origin: OriginFor<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            ensure!(T::IsOpen::is_open(), Error::<T>::RegistrarClosed);

            let label_node = OwnerToPrimaryName::<T>::get(&caller)
                .ok_or(Error::<T>::NoCanonicalName)?;

            RegistrarInfos::<T>::mutate(label_node, |info| -> DispatchResult {
                let info = info.as_mut().ok_or(Error::<T>::NotExistOrOccupied)?;
                let now = T::NowProvider::now();
                let grace_period = T::GracePeriod::get();
                ensure!(now <= info.expire + grace_period, Error::<T>::NotRenewable);
                let price = T::PriceOracle::registration_fee(info.label_len as usize)
                    .ok_or(ArithmeticError::Overflow)?;
                let imbalance = T::Currency::withdraw(
                    &caller,
                    price,
                    polkadot_sdk::frame_support::traits::WithdrawReasons::FEE,
                    ExistenceRequirement::KeepAlive,
                )?;
                drop(imbalance);
                info.expire = now
                    .checked_add(&T::MaxRegistrationDuration::get())
                    .ok_or(ArithmeticError::Overflow)?;
                Self::deposit_event(Event::<T>::NameRenewed {
                    node: label_node,
                    owner: caller.clone(),
                    expire: info.expire,
                    fee: price,
                });
                Ok(())
            })
        }
        /// Transfer the caller's canonical name to another account.
        ///
        /// Ensure: The recipient must not already hold a valid canonical name.
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::transfer())]
        #[polkadot_sdk::frame_support::transactional]
        pub fn transfer(
            origin: OriginFor<T>,
            to: <T::Lookup as StaticLookup>::Source,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let to = T::Lookup::lookup(to)?;

            ensure!(T::IsOpen::is_open(), Error::<T>::RegistrarClosed);

            // Derive the node from the caller's canonical name — only their registered
            // name is transferable through this extrinsic.
            let node = OwnerToPrimaryName::<T>::get(&who)
                .ok_or(Error::<T>::NoCanonicalName)?;

            if let Some(info) = RegistrarInfos::<T>::get(node) {
                let now = T::NowProvider::now();
                ensure!(
                    info.expire + T::GracePeriod::get() > now,
                    Error::<T>::NotOwned
                );
            }
            // Prevent transfer to an account that already holds any name.
            if let Some(existing) = OwnerToPrimaryName::<T>::get(&to) {
                let now = T::NowProvider::now();
                let still_valid = RegistrarInfos::<T>::get(existing)
                    .map(|info| now <= info.expire + T::GracePeriod::get())
                    .unwrap_or(false);
                if still_valid {
                    return Err(Error::<T>::AlreadyHasCanonicalName.into());
                }
                OwnerToPrimaryName::<T>::remove(&to);
            }
            ensure!(!T::Registry::has_active_subname(&to), Error::<T>::AlreadyHoldsSubdomain);

            T::Registry::transfer(&who, &to, node)?;

            OwnerToPrimaryName::<T>::remove(&who);
            OwnerToPrimaryName::<T>::insert(&to, node);

            Ok(())
        }
        /// Release the caller's canonical name back to the pool.
        ///
        /// The name NFT is burned and the name becomes available for registration again.
        /// After releasing, the caller may register a new canonical name.
        ///
        /// Ensure: The caller must currently own the canonical name being released,
        /// and the name must have no active subdomains.
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::release_name())]
        #[polkadot_sdk::frame_support::transactional]
        pub fn release_name(origin: OriginFor<T>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            // Derive the node from the caller's canonical name — only their registered
            // name can be released through this extrinsic.
            let node = OwnerToPrimaryName::<T>::get(&caller)
                .ok_or(Error::<T>::NoCanonicalName)?;

            OwnerToPrimaryName::<T>::remove(&caller);
            T::Registry::burn(caller, node)?;

            Ok(())
        }

        /// Offer a subdomain to a specific account.
        ///
        /// Callable only by the owner of the canonical parent name.
        /// Creates a SubnameRecord in the `Offered` state addressed to `target`.
        /// The target must call `accept_subdomain` or `reject_subdomain` to act on it.
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::offer_subdomain(label.len() as u32))]
        #[polkadot_sdk::frame_support::transactional]
        pub fn offer_subdomain(
            origin: OriginFor<T>,
            label: Vec<u8>,
            target: <T::Lookup as StaticLookup>::Source,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let target = T::Lookup::lookup(target)?;

            ensure!(T::IsOpen::is_open(), Error::<T>::RegistrarClosed);

            let parent_node = OwnerToPrimaryName::<T>::get(&caller)
                .ok_or(Error::<T>::NoCanonicalName)?;

            // Parent must be live (not expired).
            let now = T::NowProvider::now();
            let info = RegistrarInfos::<T>::get(parent_node)
                .ok_or(Error::<T>::NotExistOrOccupied)?;
            ensure!(now < info.expire, Error::<T>::NotUseable);

            let (sub_label, _) = Label::new_with_len(&label).ok_or(Error::<T>::ParseLabelFailed)?;
            let label_node = sub_label.encode_with_node(&parent_node);

            let bounded_label: polkadot_sdk::frame_support::BoundedVec<
                u8,
                polkadot_sdk::sp_core::ConstU32<63>,
            > = label.clone().try_into().map_err(|_| Error::<T>::LabelTooLong)?;

            T::Registry::offer_subname(parent_node, label_node, bounded_label, target.clone(), info.capacity)?;

            Self::deposit_event(Event::<T>::SubnameOffered {
                parent: parent_node,
                subnode: label_node,
                label,
                target,
            });

            Ok(())
        }

        /// Accept a pending subdomain offer.
        ///
        /// Callable only by the account that was specified as the target in the offer.
        /// Flips state from `Offered` to `Active` and writes the SS58 record.
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::accept_subdomain())]
        #[polkadot_sdk::frame_support::transactional]
        pub fn accept_subdomain(
            origin: OriginFor<T>,
            parent: Vec<u8>,
            label: Vec<u8>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let (parent_label, _) =
                Label::new_with_len(&parent).ok_or(Error::<T>::ParseLabelFailed)?;
            let parent_node = parent_label.encode_with_node(&T::BaseNode::get());

            let (sub_label, _) =
                Label::new_with_len(&label).ok_or(Error::<T>::ParseLabelFailed)?;
            let label_node = sub_label.encode_with_node(&parent_node);

            // Parent must still be live.
            let now = T::NowProvider::now();
            let info = RegistrarInfos::<T>::get(parent_node)
                .ok_or(Error::<T>::NotExistOrOccupied)?;
            ensure!(now < info.expire, Error::<T>::NotUseable);

            T::Registry::accept_subname_offer(label_node, &caller)?;

            T::Ss58Updater::update_ss58(label_node, &caller)?;

            Self::deposit_event(Event::<T>::SubnameAccepted {
                subnode: label_node,
                target: caller,
            });

            Ok(())
        }

        /// Reject a pending subdomain offer.
        ///
        /// Callable only by the account that was specified as the target in the offer.
        /// Flips state from `Offered` to `Rejected` so the offerer can see the outcome.
        /// The offerer must call `revoke_subdomain` to clean up the record.
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::reject_subdomain())]
        pub fn reject_subdomain(
            origin: OriginFor<T>,
            parent: Vec<u8>,
            label: Vec<u8>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let (parent_label, _) =
                Label::new_with_len(&parent).ok_or(Error::<T>::ParseLabelFailed)?;
            let parent_node = parent_label.encode_with_node(&T::BaseNode::get());

            let (sub_label, _) =
                Label::new_with_len(&label).ok_or(Error::<T>::ParseLabelFailed)?;
            let label_node = sub_label.encode_with_node(&parent_node);

            T::Registry::reject_subname_offer(label_node, &caller)?;

            Self::deposit_event(Event::<T>::SubnameRejected {
                subnode: label_node,
                target: caller,
            });

            Ok(())
        }

        /// Revoke a subdomain (Offered, Rejected, or Active) created under the caller's canonical name.
        ///
        /// Callable only by the parent domain owner (the original offerer).
        /// Deletes the SubnameRecord entirely and decrements the parent's children counter.
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::revoke_subdomain())]
        #[polkadot_sdk::frame_support::transactional]
        pub fn revoke_subdomain(
            origin: OriginFor<T>,
            label: Vec<u8>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let parent_node = OwnerToPrimaryName::<T>::get(&caller)
                .ok_or(Error::<T>::NoCanonicalName)?;

            let (sub_label, _) =
                Label::new_with_len(&label).ok_or(Error::<T>::ParseLabelFailed)?;
            let label_node = sub_label.encode_with_node(&parent_node);

            T::Registry::revoke_subname(parent_node, label_node)?;

            Self::deposit_event(Event::<T>::SubnameRevoked {
                subnode: label_node,
                parent: parent_node,
            });

            Ok(())
        }

        /// Release an active subdomain voluntarily.
        ///
        /// Callable only by the current holder (the account that accepted the offer).
        /// Deletes the SubnameRecord and decrements the parent's children counter.
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::release_subdomain())]
        #[polkadot_sdk::frame_support::transactional]
        pub fn release_subdomain(
            origin: OriginFor<T>,
            parent: Vec<u8>,
            label: Vec<u8>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let (parent_label, _) =
                Label::new_with_len(&parent).ok_or(Error::<T>::ParseLabelFailed)?;
            let parent_node = parent_label.encode_with_node(&T::BaseNode::get());

            let (sub_label, _) =
                Label::new_with_len(&label).ok_or(Error::<T>::ParseLabelFailed)?;
            let label_node = sub_label.encode_with_node(&parent_node);

            let parent_from_record = T::Registry::release_subname(label_node, &caller)?;

            Self::deposit_event(Event::<T>::SubnameReleased {
                subnode: label_node,
                parent: parent_from_record,
            });

            Ok(())
        }
    }
}

use crate::traits::{Label, Official, OriginRecorder, Registry};
use polkadot_sdk::frame_support::{
    dispatch::DispatchResult,
    traits::{Currency, Get, Time},
};
use polkadot_sdk::sp_runtime::{
    traits::{CheckedAdd, SaturatedConversion, Zero},
    ArithmeticError,
};
use polkadot_sdk::sp_weights::Weight;
use polkadot_sdk::sp_std::vec::Vec;

pub trait WeightInfo {
    fn offer_subdomain(len: u32) -> Weight;
    fn accept_subdomain() -> Weight;
    fn reject_subdomain() -> Weight;
    fn revoke_subdomain() -> Weight;
    fn release_subdomain() -> Weight;
    fn register(len: u32) -> Weight;
    fn renew() -> Weight;
    fn transfer() -> Weight;
    fn add_reserved() -> Weight;
    fn remove_reserved() -> Weight;
    fn release_name() -> Weight;
}

impl<T: Config> crate::traits::Registrar for Pallet<T> {
    type Balance = BalanceOf<T>;
    type AccountId = T::AccountId;
    type Moment = T::Moment;

    fn check_expires_registrable(node: DomainHash) -> polkadot_sdk::sp_runtime::DispatchResult {
        let now = T::NowProvider::now();

        let expire = RegistrarInfos::<T>::get(node)
            .ok_or(Error::<T>::NotExistOrOccupied)?
            .expire;

        polkadot_sdk::frame_support::ensure!(now > expire + T::GracePeriod::get(), Error::<T>::Occupied);

        Ok(())
    }

    fn check_expires_renewable(node: DomainHash) -> polkadot_sdk::sp_runtime::DispatchResult {
        let now = T::NowProvider::now();

        let expire = RegistrarInfos::<T>::get(node)
            .ok_or(Error::<T>::NotExistOrOccupied)?
            .expire;

        polkadot_sdk::frame_support::ensure!(
            now < expire + T::GracePeriod::get(),
            Error::<T>::NotRenewable
        );

        Ok(())
    }

    fn check_expires_useable(node: DomainHash) -> polkadot_sdk::sp_runtime::DispatchResult {
        let now = T::NowProvider::now();

        let expire = RegistrarInfos::<T>::get(node)
            .ok_or(Error::<T>::NotExistOrOccupied)?
            .expire;

        polkadot_sdk::frame_support::ensure!(now < expire, Error::<T>::NotUseable);

        Ok(())
    }

    fn clear_registrar_info(
        node: DomainHash,
        _owner: &Self::AccountId,
    ) -> polkadot_sdk::sp_runtime::DispatchResult {
        RegistrarInfos::<T>::try_mutate_exists(node, |maybe_info| -> DispatchResult {
            *maybe_info = None;
            Ok(())
        })
    }

    fn for_redeem_code(
        name: Vec<u8>,
        to: Self::AccountId,
        duration: Self::Moment,
        label: Label,
    ) -> DispatchResult {
        let official = T::Official::get_official_account()?;
        let now = T::NowProvider::now();
        let duration = duration.min(T::MaxRegistrationDuration::get());
        let expire = now
            .checked_add(&duration)
            .ok_or(ArithmeticError::Overflow)?;
        // 防止计算结果溢出
        polkadot_sdk::frame_support::ensure!(
            expire + T::GracePeriod::get() > now + T::GracePeriod::get(),
            ArithmeticError::Overflow
        );

        // Enforce one name per address.
        if let Some(existing_node) = OwnerToPrimaryName::<T>::get(&to) {
            let still_valid = RegistrarInfos::<T>::get(existing_node)
                .map(|info| now <= info.expire + T::GracePeriod::get())
                .unwrap_or(false);
            if still_valid {
                return Err(Error::<T>::AlreadyHasCanonicalName.into());
            }
            OwnerToPrimaryName::<T>::remove(&to);
        }
        polkadot_sdk::frame_support::ensure!(
            !T::Registry::has_active_subname(&to),
            Error::<T>::AlreadyHoldsSubdomain
        );

        let base_node = T::BaseNode::get();
        let label_node = label.encode_with_node(&base_node);

        T::Registry::mint_subname(
            &official,
            base_node,
            label_node,
            to.clone(),
            0,
            |maybe_pre_owner| -> DispatchResult {
                if let Some(pre_owner) = maybe_pre_owner {
                    if OwnerToPrimaryName::<T>::get(pre_owner) == Some(label_node) {
                        OwnerToPrimaryName::<T>::remove(pre_owner);
                    }
                }
                let current_block = polkadot_sdk::frame_system::Pallet::<T>::block_number().saturated_into::<u32>();
                RegistrarInfos::<T>::mutate(label_node, |info| -> DispatchResult {
                    if let Some(info) = info.as_mut() {
                        info.register_fee = Zero::zero();
                        info.expire = expire;
                        info.last_block = current_block;
                    } else {
                        let _ = info.insert(RegistrarInfoOf::<T> {
                            register_fee: Zero::zero(),
                            expire,
                            capacity: T::DefaultCapacity::get(),
                            label_len: name.len() as u32,
                            last_block: current_block,
                        });
                    }
                    Ok(())
                })?;
                Ok(())
            },
        )?;
        // Record this as the recipient's canonical name.
        OwnerToPrimaryName::<T>::insert(&to, label_node);

        let parent_hash: [u8; 32] = polkadot_sdk::frame_system::Pallet::<T>::parent_hash()
            .as_ref()
            .try_into()
            .unwrap_or([0u8; 32]);
        T::OriginRecorder::record_origin(label_node, parent_hash)?;

        Self::deposit_event(Event::<T>::NameRegistered {
            name,
            node: label_node,
            owner: to,
            expire,
            fee: Zero::zero(),
        });

        Ok(())
    }

    fn basenode() -> DomainHash {
        T::BaseNode::get()
    }

    fn has_valid_canonical_name(account: &Self::AccountId) -> bool {
        let Some(node) = OwnerToPrimaryName::<T>::get(account) else { return false };
        let now = T::NowProvider::now();
        RegistrarInfos::<T>::get(node)
            .map(|info| now <= info.expire + T::GracePeriod::get())
            .unwrap_or(false)
    }
}

impl WeightInfo for () {
    fn offer_subdomain(_len: u32) -> Weight { Weight::zero() }
    fn accept_subdomain() -> Weight { Weight::zero() }
    fn reject_subdomain() -> Weight { Weight::zero() }
    fn revoke_subdomain() -> Weight { Weight::zero() }
    fn release_subdomain() -> Weight { Weight::zero() }

    fn register(_len: u32) -> Weight {
        Weight::zero()
    }

    fn renew() -> Weight {
        Weight::zero()
    }

    fn transfer() -> Weight {
        Weight::zero()
    }

    fn add_reserved() -> Weight {
        Weight::zero()
    }

    fn remove_reserved() -> Weight {
        Weight::zero()
    }
    fn release_name() -> Weight {
        Weight::zero()
    }
}

impl<T: Config> Pallet<T> {
    pub fn get_info(id: DomainHash) -> Option<RegistrarInfoOf<T>> {
        RegistrarInfos::<T>::get(id)
    }
    pub fn all() -> Vec<(DomainHash, RegistrarInfoOf<T>)> {
        RegistrarInfos::<T>::iter().collect::<Vec<_>>()
    }
}

impl<T: Config> crate::traits::NameRegistry for Pallet<T> {
    type AccountId = T::AccountId;

    fn canonical_name(account: &T::AccountId) -> Option<DomainHash> {
        OwnerToPrimaryName::<T>::get(account)
    }

    fn owner_of(node: DomainHash) -> Option<T::AccountId> {
        T::Registry::owner_of(node)
    }

    fn transfer_name(from: &T::AccountId, to: &T::AccountId, node: DomainHash) -> polkadot_sdk::sp_runtime::DispatchResult {
        // Prevent transfer to an account that already holds any name.
        if let Some(existing) = OwnerToPrimaryName::<T>::get(to) {
            let now = T::NowProvider::now();
            let still_valid = RegistrarInfos::<T>::get(existing)
                .map(|info| now <= info.expire + T::GracePeriod::get())
                .unwrap_or(false);
            if still_valid {
                return Err(Error::<T>::AlreadyHasCanonicalName.into());
            }
            OwnerToPrimaryName::<T>::remove(to);
        }
        polkadot_sdk::frame_support::ensure!(
            !T::Registry::has_active_subname(to),
            Error::<T>::AlreadyHoldsSubdomain
        );
        T::Registry::transfer(from, to, node)?;
        if OwnerToPrimaryName::<T>::get(from) == Some(node) {
            OwnerToPrimaryName::<T>::remove(from);
        }
        OwnerToPrimaryName::<T>::insert(to, node);
        Ok(())
    }
}
