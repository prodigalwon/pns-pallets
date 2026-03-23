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

// External crates imports
use alloc::vec::Vec;
use frame_support::{
	genesis_builder_helper::{build_state, get_preset},
	weights::Weight,
};
use pallet_grandpa::AuthorityId as GrandpaId;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::{
	traits::{Block as BlockT, NumberFor},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult,
};
use sp_session::OpaqueGeneratedSessionKeys;
use sp_version::RuntimeVersion;

// Local module imports
use crate::InherentDataExt;
use super::{
	AccountId, Aura, Balance, Block, Executive, Grandpa, Nonce, Runtime,
	RuntimeCall, RuntimeGenesisConfig, SessionKeys, System, TransactionPayment, VERSION,
};

impl_runtime_apis! {
	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: <Block as BlockT>::LazyBlock) {
			Executive::execute_block(block);
		}

		fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> Vec<u32> {
			Runtime::metadata_versions()
		}
	}

	impl frame_support::view_functions::runtime_api::RuntimeViewFunction<Block> for Runtime {
		fn execute_view_function(id: frame_support::view_functions::ViewFunctionId, input: Vec<u8>) -> Result<Vec<u8>, frame_support::view_functions::ViewFunctionDispatchError> {
			Runtime::execute_view_function(id, input)
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: <Block as BlockT>::LazyBlock,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			pallet_aura::Authorities::<Runtime>::get().into_inner()
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(owner: Vec<u8>, seed: Option<Vec<u8>>) -> OpaqueGeneratedSessionKeys {
			SessionKeys::generate(&owner, seed).into()
		}

		fn decode_session_keys(
			encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}

	impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
		fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
			Grandpa::grandpa_authorities()
		}

		fn current_set_id() -> sp_consensus_grandpa::SetId {
			Grandpa::current_set_id()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			_equivocation_proof: sp_consensus_grandpa::EquivocationProof<
				<Block as BlockT>::Hash,
				NumberFor<Block>,
			>,
			_key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			None
		}

		fn generate_key_ownership_proof(
			_set_id: sp_consensus_grandpa::SetId,
			_authority_id: GrandpaId,
		) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
			// NOTE: this is the only implementation possible since we've
			// defined our key owner proof type as a bottom type (i.e. a type
			// with no values).
			None
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
			System::account_nonce(account)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
		fn query_info(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
		for Runtime
	{
		fn query_call_info(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_call_info(call, len)
		}
		fn query_call_fee_details(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_call_fee_details(call, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{baseline, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;
			use frame_system_benchmarking::extensions::Pallet as SystemExtensionsBench;
			use baseline::Pallet as BaselineBench;
			use super::*;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();

			(list, storage_info)
		}

		#[allow(non_local_definitions)]
		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, alloc::string::String> {
			use frame_benchmarking::{baseline, BenchmarkBatch};
			use sp_storage::TrackedStorageKey;
			use frame_system_benchmarking::Pallet as SystemBench;
			use frame_system_benchmarking::extensions::Pallet as SystemExtensionsBench;
			use baseline::Pallet as BaselineBench;
			use super::*;

			impl frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}

			use frame_support::traits::WhitelistedStorageKeys;
			let whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here. If any of the pre/post migration checks fail, we shall stop
			// right here and right now.
			let weight = Executive::try_runtime_upgrade(checks).unwrap();
			(weight, super::configs::RuntimeBlockWeights::get().max_block)
		}

		fn execute_block(
			block: <Block as BlockT>::LazyBlock,
			state_root_check: bool,
			signature_check: bool,
			select: frame_try_runtime::TryStateSelect
		) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			Executive::try_execute_block(block, state_root_check, signature_check, select).expect("execute-block failed")
		}
	}

	impl pns_runtime_api::PnsStorageApi<Block, u64, Balance, AccountId> for Runtime {
		fn get_info(id: pns_types::DomainHash) -> Option<pns_types::NameRecord<AccountId, u64, Balance>> {
			use frame_support::traits::Time;
			// Names in offered state are not yet active — return null until the recipient accepts.
			if pns_registrar::registrar::OfferedNames::<Runtime>::contains_key(id) {
				return None;
			}
			let info = pns_registrar::registrar::Pallet::<Runtime>::get_info(id)?;
			// Expired registrations resolve to None — the name is up for grabs.
			if pallet_timestamp::Pallet::<Runtime>::now() >= info.expire {
				return None;
			}
			let token = pns_registrar::nft::Pallet::<Runtime>::tokens(0u32, id)?;
			let for_sale = pns_marketplace::Listings::<Runtime>::contains_key(id);
			Some(pns_types::NameRecord {
				owner: token.owner,
				expire: info.expire,
				capacity: info.capacity,
				register_fee: info.register_fee,
				for_sale,
				last_block: info.last_block,
				read_block_number: 0,
				read_block_hash: Default::default(),
			})
		}
		fn all() -> Vec<(pns_types::DomainHash, pns_types::RegistrarInfo<u64, Balance>)> {
			pns_registrar::registrar::Pallet::<Runtime>::all()
		}
		fn lookup(id: pns_types::DomainHash, record_types: Vec<pns_types::ddns::codec_type::RecordType>) -> Vec<(pns_types::ddns::codec_type::RecordType, Vec<u8>)> {
			pns_resolvers::resolvers::Pallet::<Runtime>::lookup(id, record_types)
		}
		fn get_listing(name: Vec<u8>) -> Option<pns_types::ListingInfo<AccountId, Balance, u64>> {
			use pns_registrar::traits::Label;
			let (label, _) = Label::new_with_len(&name)?;
			let node = label.encode_with_node(&pns_types::NATIVE_BASENODE);
			let l = pns_marketplace::Listings::<Runtime>::get(node)?;
			Some(pns_types::ListingInfo {
				seller: l.seller,
				price: l.price,
				expires_at: l.expires_at,
				read_block_number: 0,
				read_block_hash: Default::default(),
			})
		}
		fn resolve_name(name: Vec<u8>) -> Option<pns_types::NameRecord<AccountId, u64, Balance>> {
			use frame_support::traits::Time;
			use pns_registrar::traits::Label;
			let (label, _) = Label::new_with_len(&name)?;
			let base_node = pns_types::NATIVE_BASENODE;
			let node = label.encode_with_node(&base_node);
			// Names in offered state are not yet active — return null until the recipient accepts.
			if pns_registrar::registrar::OfferedNames::<Runtime>::contains_key(node) {
				return None;
			}
			let info = pns_registrar::registrar::Pallet::<Runtime>::get_info(node)?;
			// Expired registrations resolve to None — the name is up for grabs.
			if pallet_timestamp::Pallet::<Runtime>::now() >= info.expire {
				return None;
			}
			let token = pns_registrar::nft::Pallet::<Runtime>::tokens(0u32, node)?;
			let for_sale = pns_marketplace::Listings::<Runtime>::contains_key(node);
			Some(pns_types::NameRecord {
				owner: token.owner,
				expire: info.expire,
				capacity: info.capacity,
				register_fee: info.register_fee,
				for_sale,
				last_block: info.last_block,
				read_block_number: 0,
				read_block_hash: Default::default(),
			})
		}
		fn lookup_by_name(name: Vec<u8>, record_types: Vec<pns_types::ddns::codec_type::RecordType>) -> Vec<(pns_types::ddns::codec_type::RecordType, Vec<u8>)> {
			let node = pns_types::parse_name_to_node(&name, &pns_types::NATIVE_BASENODE)
				.unwrap_or_default();
			pns_resolvers::resolvers::Pallet::<Runtime>::lookup(node, record_types)
		}
		fn get_subname(node: pns_types::DomainHash) -> Option<pns_types::SubnameRecord<AccountId>> {
			pns_registrar::registry::SubnameRecords::<Runtime>::get(node)
		}
		fn account_dashboard(owner: AccountId) -> pns_types::AccountDashboard {
			let primary_name = pns_registrar::registrar::OwnerToPrimaryName::<Runtime>::get(&owner);
			let subnames = pns_registrar::registry::AccountToSubnames::<Runtime>::iter_prefix(&owner)
				.map(|(hash, _)| hash)
				.collect();
			let pending_subname_offers = pns_registrar::registry::OfferedToAccount::<Runtime>::iter_prefix(&owner)
				.map(|(hash, _)| hash)
				.collect();
			let pending_name_offers = pns_registrar::registrar::OfferedNames::<Runtime>::iter()
				.filter_map(|(node, record)| {
					if record.recipient == owner { Some(node) } else { None }
				})
				.collect();
			pns_types::AccountDashboard {
				primary_name,
				subnames,
				pending_subname_offers,
				pending_name_offers,
			}
		}
	}

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
			get_preset::<RuntimeGenesisConfig>(id, crate::genesis_config_presets::get_preset)
		}

		fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
			crate::genesis_config_presets::preset_names()
		}
	}
}
