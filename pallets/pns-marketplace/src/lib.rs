#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[polkadot_sdk::frame_support::pallet]
pub mod pallet {
    use codec::{DecodeWithMemTracking, Encode, Decode, MaxEncodedLen};
    use polkadot_sdk::frame_support::{
        pallet_prelude::*,
        traits::{Currency, ExistenceRequirement, Time, WithdrawReasons},
    };
    use polkadot_sdk::frame_system::{ensure_signed, pallet_prelude::*};
    use polkadot_sdk::sp_runtime::traits::{AtLeast32Bit, MaybeSerializeDeserialize};
    use pns_types::DomainHash;
    use pns_registrar::traits::{Label, NameRegistry, OriginRecorder, RecordCleaner, Ss58Updater};
    use polkadot_sdk::sp_runtime::Saturating;
    use polkadot_sdk::sp_std::vec::Vec;
    use crate::WeightInfo;
    use scale_info::TypeInfo;

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as polkadot_sdk::frame_system::Config>::AccountId>>::Balance;

    #[pallet::config]
    pub trait Config: polkadot_sdk::frame_system::Config {
        /// The currency used for listing prices and protocol fees.
        type Currency: Currency<Self::AccountId>;

        /// Moment type — millisecond timestamps, same unit as pallet_timestamp.
        type Moment: AtLeast32Bit
            + Parameter
            + Default
            + Copy
            + MaxEncodedLen
            + MaybeSerializeDeserialize;

        /// Provider of the current on-chain time.
        type NowProvider: Time<Moment = Self::Moment>;

        /// Access to name ownership and transfer operations.
        type NameRegistry: pns_registrar::traits::NameRegistry<AccountId = Self::AccountId>;

        /// Writes the SS58 (owner account) record after a name is purchased.
        type Ss58Updater: Ss58Updater<AccountId = Self::AccountId>;

        /// Clears non-SS58 DNS records when a name is purchased.
        type RecordCleaner: RecordCleaner;

        /// Writes the ORIGIN record (block hash) when a name is purchased.
        type OriginRecorder: OriginRecorder;

        /// The base namehash for the TLD (e.g. the namehash of "dot").
        #[pallet::constant]
        type BaseNode: Get<DomainHash>;

        /// Protocol fee in basis points (e.g. 200 = 2%). Burned from the seller's proceeds.
        #[pallet::constant]
        type ProtocolFeeBps: Get<u32>;

        type WeightInfo: WeightInfo;
    }

    /// An active sale listing.
    #[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
    #[derive(serde::Serialize, serde::Deserialize)]
    pub struct Listing<AccountId, Balance, Moment> {
        /// Account that created the listing and will receive the proceeds.
        pub seller: AccountId,
        /// Asking price in the native currency.
        pub price: Balance,
        /// Millisecond timestamp after which this listing is no longer valid.
        pub expires_at: Moment,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// Active listings indexed by the name's namehash.
    #[pallet::storage]
    pub type Listings<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        DomainHash,
        Listing<T::AccountId, BalanceOf<T>, T::Moment>,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A name was listed for sale.
        Listed {
            node: DomainHash,
            seller: T::AccountId,
            price: BalanceOf<T>,
            expires_at: T::Moment,
        },
        /// A listing was cancelled by the owner.
        Delisted {
            node: DomainHash,
            seller: T::AccountId,
        },
        /// A name was sold. `fee` was burned; seller received `price - fee`.
        Sold {
            node: DomainHash,
            seller: T::AccountId,
            buyer: T::AccountId,
            price: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// A name was purchased as a gift for `recipient` and is awaiting their acceptance.
        NameBoughtForRecipient {
            node: DomainHash,
            seller: T::AccountId,
            buyer: T::AccountId,
            recipient: T::AccountId,
            price: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Caller has no canonical name registered.
        NoCanonicalName,
        /// This name is already listed for sale.
        AlreadyListed,
        /// No active listing found for this name.
        NotListed,
        /// The listing's expiry time is in the past.
        ListingExpired,
        /// The listed seller no longer owns this name.
        SellerNoLongerOwns,
        /// Expiry must be in the future.
        ExpiryNotInFuture,
        /// The provided name label is invalid (empty, too long, or contains illegal characters).
        InvalidName,
        /// The buyer and the seller cannot be the same account.
        BuyerIsSeller,
        /// The buyer cannot also be the intended gift recipient.
        BuyerIsRecipient,
        /// The seller cannot be the gift recipient (no self-gifting).
        SellerIsRecipient,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// List the caller's canonical name for sale at `price`, expiring at `expires_at`.
        ///
        /// The name remains usable by the owner while listed — there is no escrow lock.
        /// If the owner transfers or releases the name before a buyer appears, the listing
        /// becomes stale and any `buy_name` attempt will fail.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::create_listing())]
        pub fn create_listing(
            origin: OriginFor<T>,
            price: BalanceOf<T>,
            expires_at: T::Moment,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let now = T::NowProvider::now();
            ensure!(expires_at > now, Error::<T>::ExpiryNotInFuture);
            let node = T::NameRegistry::canonical_name(&who)
                .ok_or(Error::<T>::NoCanonicalName)?;
            ensure!(!Listings::<T>::contains_key(node), Error::<T>::AlreadyListed);
            Listings::<T>::insert(node, Listing { seller: who.clone(), price, expires_at });
            Self::deposit_event(Event::<T>::Listed { node, seller: who, price, expires_at });
            Ok(())
        }

        /// Cancel the caller's active listing.
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::cancel_listing())]
        pub fn cancel_listing(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let node = T::NameRegistry::canonical_name(&who)
                .ok_or(Error::<T>::NoCanonicalName)?;
            ensure!(Listings::<T>::contains_key(node), Error::<T>::NotListed);
            Listings::<T>::remove(node);
            Self::deposit_event(Event::<T>::Delisted { node, seller: who });
            Ok(())
        }

        /// Purchase a listed name. The buyer passes the plain DNS label (e.g. `b"alice"`);
        /// the pallet resolves it to the namehash internally. The buyer pays `listing.price`;
        /// the protocol fee is burned from the seller's proceeds; the name NFT is transferred atomically.
        ///
        /// Pass `recipient = Some(account)` to buy the name as a gift for someone else.
        /// The name enters an "offered" state — lookups return null — until the recipient
        /// calls `accept_offered_name` on the registrar pallet. The recipient must not
        /// already hold a canonical name or an active subdomain.
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::buy_name())]
        #[polkadot_sdk::frame_support::transactional]
        pub fn buy_name(origin: OriginFor<T>, name: Vec<u8>, recipient: Option<T::AccountId>) -> DispatchResult {
            let buyer = ensure_signed(origin)?;
            let label = Label::new(&name).ok_or(Error::<T>::InvalidName)?;
            let node = label.encode_with_node(&T::BaseNode::get());
            let listing = Listings::<T>::get(node).ok_or(Error::<T>::NotListed)?;
            ensure!(buyer != listing.seller, Error::<T>::BuyerIsSeller);
            ensure!(listing.expires_at > T::NowProvider::now(), Error::<T>::ListingExpired);
            ensure!(
                Some(listing.seller.clone()) == T::NameRegistry::owner_of(node),
                Error::<T>::SellerNoLongerOwns
            );

            // Transfer full price from buyer to seller.
            T::Currency::transfer(
                &buyer,
                &listing.seller,
                listing.price,
                ExistenceRequirement::KeepAlive,
            )?;

            // Burn the protocol fee from the seller's proceeds.
            let fee: BalanceOf<T> = listing.price
                .saturating_mul(T::ProtocolFeeBps::get().into())
                / 10_000u32.into();
            let imbalance = T::Currency::withdraw(
                &listing.seller,
                fee,
                WithdrawReasons::FEE,
                ExistenceRequirement::KeepAlive,
            )?;
            drop(imbalance); // burned

            Listings::<T>::remove(node);

            if let Some(recip) = recipient {
                // Gift purchase path: transfer to recipient in "offered" state.
                ensure!(recip != buyer, Error::<T>::BuyerIsRecipient);
                ensure!(recip != listing.seller, Error::<T>::SellerIsRecipient);

                T::NameRegistry::offer_bought_name(&listing.seller, &buyer, &recip, node)?;

                Self::deposit_event(Event::<T>::NameBoughtForRecipient {
                    node,
                    seller: listing.seller,
                    buyer,
                    recipient: recip,
                    price: listing.price,
                    fee,
                });
            } else {
                // Standard purchase path: transfer directly to buyer.
                T::NameRegistry::transfer_name(&listing.seller, &buyer, node)?;

                // Clear stale DNS records from the seller, then write the buyer's SS58 and ORIGIN.
                T::RecordCleaner::clear_records_except_ss58(node);
                T::Ss58Updater::update_ss58(node, &buyer)?;
                let parent_hash: [u8; 32] = polkadot_sdk::frame_system::Pallet::<T>::parent_hash()
                    .as_ref()
                    .try_into()
                    .unwrap_or([0u8; 32]);
                T::OriginRecorder::record_origin(node, parent_hash)?;

                Self::deposit_event(Event::<T>::Sold {
                    node,
                    seller: listing.seller,
                    buyer,
                    price: listing.price,
                    fee,
                });
            }
            Ok(())
        }
    }
}

use polkadot_sdk::sp_weights::Weight;
use pns_types::DomainHash;

pub trait WeightInfo {
    fn create_listing() -> Weight;
    fn cancel_listing() -> Weight;
    fn buy_name() -> Weight;
}

impl WeightInfo for () {
    fn create_listing() -> Weight { Weight::zero() }
    fn cancel_listing() -> Weight { Weight::zero() }
    fn buy_name() -> Weight { Weight::zero() }
}

impl<T: Config> Pallet<T> {
    /// Returns the active listing for `node`, if any.
    pub fn listing(node: DomainHash) -> Option<Listing<T::AccountId, BalanceOf<T>, T::Moment>> {
        pallet::Listings::<T>::get(node)
    }
}
