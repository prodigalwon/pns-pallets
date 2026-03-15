// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

// Substrate and Polkadot dependencies
use frame_support::{
	derive_impl,
	dispatch::DispatchClass,
	parameter_types,
	traits::{ConstBool, ConstU128, ConstU32, ConstU64, ConstU8, VariantCountOf},
	weights::{
		constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
		IdentityFee, Weight,
	},
};
use frame_system::limits::{BlockLength, BlockWeights};
use pallet_transaction_payment::{ConstFeeMultiplier, FungibleAdapter, Multiplier};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{traits::One, Perbill};
use sp_version::RuntimeVersion;

// Local module imports
use super::{
	AccountId, Aura, Balance, Balances, Block, BlockNumber, Hash, MultiSignature, Nonce,
	PalletInfo, Runtime, RuntimeCall, RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason,
	RuntimeOrigin, RuntimeTask, System, Timestamp, DAYS, EXISTENTIAL_DEPOSIT, MILLI_SECS_PER_BLOCK,
	SLOT_DURATION, VERSION,
};

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 2400;
	pub const Version: RuntimeVersion = VERSION;

	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
		Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
		NORMAL_DISPATCH_RATIO,
	);
	pub RuntimeBlockLength: BlockLength = BlockLength::builder()
		.max_length(5 * 1024 * 1024)
		.modify_max_length_for_class(DispatchClass::Normal, |m| *m = NORMAL_DISPATCH_RATIO * *m)
		.build();
	pub const SS58Prefix: u8 = 42;
}

/// All migrations of the runtime, aside from the ones declared in the pallets.
///
/// This can be a tuple of types, each implementing `OnRuntimeUpgrade`.
#[allow(unused_parens)]
type SingleBlockMigrations = ();

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type SingleBlockMigrations = SingleBlockMigrations;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<32>;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type WeightInfo = ();
	type MaxAuthorities = ConstU32<32>;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<0>;

	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

parameter_types! {
	pub FeeMultiplier: Multiplier = Multiplier::one();
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = IdentityFee<Balance>;
	type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
	type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

/// Configure the pallet-template in pallets/template.
impl pallet_template::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_template::weights::SubstrateWeight<Runtime>;
}
// ================================================================
// PNS Configuration
// ================================================================

use pns_types::DomainHash;
use pns_registrar::traits::Registrar as RegistrarTrait;

/// `EnsureRoot`-equivalent that satisfies `Success = AccountId`.
///
/// Pallet `ManagerOrigin` traits require `EnsureOrigin<…, Success = AccountId>`.
/// `frame_system::EnsureRoot` has `Success = ()`, so this wrapper adapts it by
/// returning `AccountId::default()` (the all-zeroes address) on success.
/// The account is only used in informational events; all calls are still
/// gated behind `RuntimeOrigin::Root` (i.e. sudo in a solo chain).
pub struct EnsureRootAsAccountId;
impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for EnsureRootAsAccountId {
    type Success = AccountId;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        frame_system::EnsureRoot::<AccountId>::try_origin(o)
            .map(|_| AccountId::new([0u8; 32]))
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        Ok(frame_system::RawOrigin::Root.into())
    }
}

parameter_types! {
    pub const GracePeriod: u64 = 30 * DAYS as u64 * MILLI_SECS_PER_BLOCK;
    pub const DefaultCapacity: u32 = 10;
    pub const MinRegistrationDuration: u64 = 28 * DAYS as u64 * MILLI_SECS_PER_BLOCK;
    pub const MaxRegistrationDuration: u64 = 365 * DAYS as u64 * MILLI_SECS_PER_BLOCK;
    pub const BaseNode: DomainHash = pns_types::NATIVE_BASENODE;
    pub const OffchainPrefix: &'static [u8] = b"pns/";
    pub const MaxContentLen: u32 = 1024;
}

// NFT pallet (base layer for domain ownership)
impl pns_registrar::nft::Config for Runtime {
    type ClassId = u32;
    type TotalId = u128;
    type TokenId = DomainHash;
    type ClassData = ();
    type TokenData = pns_types::Record;
    type MaxClassMetadata = ConstU32<0>;
    type MaxTokenMetadata = ConstU32<0>;
}

// Price oracle pallet
impl pns_registrar::price_oracle::Config for Runtime {
    type Currency = Balances;
    type Moment = u64;
    type ExchangeRate = pns_registrar::price_oracle::Pallet<Runtime>;
    type WeightInfo = ();
    type ManagerOrigin = EnsureRootAsAccountId;
}

// Registry pallet (domain NFT management)
impl pns_registrar::registry::Config for Runtime {
    type WeightInfo = ();
    type Registrar = pns_registrar::registrar::Pallet<Runtime>;
    type ResolverId = u32;
    type ManagerOrigin = EnsureRootAsAccountId;
    type Ss58Updater = PnsSs58Updater;
    type RecordCleaner = PnsRecordCleaner;
    type OriginRecorder = PnsOriginRecorder;
}

pub struct PnsIsOpen;
impl pns_registrar::traits::IsRegistrarOpen for PnsIsOpen {
    fn is_open() -> bool { true }
}

pub struct PnsSs58Updater;
impl pns_registrar::traits::Ss58Updater for PnsSs58Updater {
    type AccountId = AccountId;
    fn update_ss58(node: DomainHash, owner: &AccountId) -> sp_runtime::DispatchResult {
        pns_resolvers::resolvers::Pallet::<Runtime>::set_ss58_record(node, owner)
    }
}

pub struct PnsOriginRecorder;
impl pns_registrar::traits::OriginRecorder for PnsOriginRecorder {
    fn record_origin(node: DomainHash, block_hash: [u8; 32]) -> sp_runtime::DispatchResult {
        pns_resolvers::resolvers::Pallet::<Runtime>::set_origin_record(node, block_hash)
    }
}

pub struct PnsRecordCleaner;
impl pns_registrar::traits::RecordCleaner for PnsRecordCleaner {
    fn clear_records_except_ss58(node: DomainHash) {
        pns_resolvers::resolvers::Pallet::<Runtime>::clear_records_except_ss58(node)
    }
    fn clear_all_records(node: DomainHash) {
        pns_resolvers::resolvers::Pallet::<Runtime>::clear_all_records(node)
    }
}

pub struct PnsRegistryChecker;
impl pns_resolvers::resolvers::RegistryChecker for PnsRegistryChecker {
    type AccountId = AccountId;
    fn check_node_useable(node: DomainHash, owner: &AccountId) -> bool {
        // Caller must own or be an operator of the domain.
        if pns_registrar::registry::Pallet::<Runtime>::verify(owner, node).is_err() {
            return false;
        }
        // For root domains (.dot names), also verify expiry.
        // Subdomains have no RegistrarInfo; their validity follows from ownership.
        pns_registrar::registrar::Pallet::<Runtime>::get_info(node)
            .map(|_| pns_registrar::registrar::Pallet::<Runtime>::check_expires_useable(node).is_ok())
            .unwrap_or(true)
    }
    fn base_node() -> DomainHash {
        BaseNode::get()
    }
}

impl pns_registrar::registrar::Config for Runtime {
    type ResolverId = u32;
    type Registry = pns_registrar::registry::Pallet<Runtime>;
    type Currency = Balances;
    type NowProvider = Timestamp;
    type Moment = u64;
    type GracePeriod = GracePeriod;
    type DefaultCapacity = DefaultCapacity;
    type BaseNode = BaseNode;
    type MinRegistrationDuration = MinRegistrationDuration;
    type MaxRegistrationDuration = MaxRegistrationDuration;
    type WeightInfo = ();
    type PriceOracle = pns_registrar::price_oracle::Pallet<Runtime>;
    type ManagerOrigin = EnsureRootAsAccountId;
    type IsOpen = PnsIsOpen;
    type Official = pns_registrar::registry::Pallet<Runtime>;
    type Ss58Updater = PnsSs58Updater;
    type OriginRecorder = PnsOriginRecorder;
    type RecordCleaner = PnsRecordCleaner;
}

impl pns_resolvers::resolvers::Config for Runtime {
    const OFFCHAIN_PREFIX: &'static [u8] = b"pns/";
    type WeightInfo = ();
    type MaxContentLen = MaxContentLen;
    type AccountIndex = u32;
    type RegistryChecker = PnsRegistryChecker;
    type Public = <MultiSignature as sp_runtime::traits::Verify>::Signer;
    type Signature = MultiSignature;
}

parameter_types! {
    /// 2% of the sale price is burned on every name sale.
    pub const MarketplaceProtocolFeeBps: u32 = 200;
}

impl pns_marketplace::Config for Runtime {
    type Currency = Balances;
    type Moment = u64;
    type NowProvider = Timestamp;
    type NameRegistry = pns_registrar::registrar::Pallet<Runtime>;
    type Ss58Updater = PnsSs58Updater;
    type RecordCleaner = PnsRecordCleaner;
    type OriginRecorder = PnsOriginRecorder;
    type BaseNode = BaseNode;
    type ProtocolFeeBps = MarketplaceProtocolFeeBps;
    type WeightInfo = ();
}