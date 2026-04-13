//! Parachain service: Cumulus collator with PNS DNS server.

use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_aura::collators::basic::{
	self as basic_aura, Params as BasicAuraParams,
};
use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use cumulus_client_service::{
	build_network, build_relay_chain_interface, prepare_node_config, start_relay_chain_tasks,
	BuildNetworkParams, CollatorSybilResistance, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use sc_consensus::ImportQueue;
use sc_network::{NetworkBackend, NetworkBlock};
use sc_service::{error::Error as ServiceError, Configuration, PartialComponents, TaskManager};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use solochain_template_runtime::{self, apis::RuntimeApi, opaque::Block};
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use std::{sync::Arc, time::Duration};
use substrate_prometheus_endpoint::Registry;

type ParachainBlockImport = TParachainBlockImport<Block, Arc<FullClient>, FullBackend>;

pub(crate) type FullClient = sc_service::TFullClient<
	Block,
	RuntimeApi,
	sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
>;
type FullBackend = sc_service::TFullBackend<Block>;

pub type Service = PartialComponents<
	FullClient,
	FullBackend,
	(),
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::TransactionPoolHandle<Block, FullClient>,
	(ParachainBlockImport, Option<Telemetry>, Option<TelemetryWorkerHandle>),
>;

fn build_import_queue(
	client: Arc<FullClient>,
	block_import: ParachainBlockImport,
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, ServiceError> {
	Ok(cumulus_client_consensus_aura::import_queue::<AuraPair, _, _, _, _, _>(
		cumulus_client_consensus_aura::ImportQueueParams {
			block_import,
			client,
			create_inherent_data_providers: move |_, _| async move {
				let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
				let slot =
					sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
						*timestamp,
						sp_consensus_aura::SlotDuration::from_millis(
							solochain_template_runtime::SLOT_DURATION,
						),
					);
				Ok((slot, timestamp))
			},
			registry: config.prometheus_registry(),
			spawner: &task_manager.spawn_essential_handle(),
			telemetry,
		},
	)?)
}

pub fn new_partial(config: &Configuration) -> Result<Service, ServiceError> {
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let executor =
		sc_service::new_wasm_executor::<sp_io::SubstrateHostFunctions>(&config.executor);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;
	let client = Arc::new(client);

	let telemetry_worker_handle = telemetry.as_ref().map(|(worker, _)| worker.handle());
	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let transaction_pool = Arc::from(
		sc_transaction_pool::Builder::new(
			task_manager.spawn_essential_handle(),
			client.clone(),
			config.role.is_authority().into(),
		)
		.with_options(config.transaction_pool.clone())
		.with_prometheus(config.prometheus_registry())
		.build(),
	);

	let block_import = ParachainBlockImport::new(client.clone(), backend.clone());

	let import_queue = build_import_queue(
		client.clone(),
		block_import.clone(),
		config,
		telemetry.as_ref().map(|x| x.handle()),
		&task_manager,
	)?;

	Ok(PartialComponents {
		backend,
		client,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		select_chain: (),
		other: (block_import, telemetry, telemetry_worker_handle),
	})
}

fn start_consensus(
	client: Arc<FullClient>,
	block_import: ParachainBlockImport,
	prometheus_registry: Option<&Registry>,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	transaction_pool: Arc<sc_transaction_pool::TransactionPoolHandle<Block, FullClient>>,
	keystore: sp_keystore::KeystorePtr,
	relay_chain_slot_duration: Duration,
	para_id: ParaId,
	collator_key: polkadot_primitives::CollatorPair,
	collator_peer_id: sc_network::PeerId,
	overseer_handle: OverseerHandle,
	announce_block: Arc<dyn Fn(solochain_template_runtime::Hash, Option<Vec<u8>>) + Send + Sync>,
) -> Result<(), ServiceError> {
	let proposer_factory = sc_basic_authorship::ProposerFactory::new(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool,
		prometheus_registry,
		telemetry.clone(),
	);

	let collator_service = CollatorService::new(
		client.clone(),
		Arc::new(task_manager.spawn_handle()),
		announce_block,
		client.clone(),
	);

	let params = BasicAuraParams {
		create_inherent_data_providers: move |_, ()| async move {
			let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
			let slot =
				sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
					*timestamp,
					sp_consensus_aura::SlotDuration::from_millis(
						solochain_template_runtime::SLOT_DURATION,
					),
				);
			Ok((slot, timestamp))
		},
		block_import,
		para_client: client,
		relay_client: relay_chain_interface,
		keystore,
		collator_key,
		collator_peer_id,
		para_id,
		overseer_handle,
		relay_chain_slot_duration,
		proposer: proposer_factory,
		collator_service,
		authoring_duration: Duration::from_millis(500),
		collation_request_receiver: None,
	};

	let fut = basic_aura::run::<Block, AuraPair, _, _, _, _, _, _>(params);
	task_manager.spawn_essential_handle().spawn("aura", None, fut);

	Ok(())
}

/// Start the parachain node, connecting to the relay chain.
pub async fn start_parachain_node<
	N: NetworkBackend<Block, <Block as sp_runtime::traits::Block>::Hash>,
>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: cumulus_client_cli::CollatorOptions,
	para_id: ParaId,
	no_dns: bool,
	dns_config: pns_ddns::DnsConfig,
) -> Result<(TaskManager, Arc<FullClient>), ServiceError> {
	let parachain_config = prepare_node_config(parachain_config);

	let PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		transaction_pool,
		other: (block_import, mut telemetry, telemetry_worker_handle),
		..
	} = new_partial(&parachain_config)?;

	let (relay_chain_interface, collator_key, _network_service, _req_receiver) =
		build_relay_chain_interface(
			polkadot_config,
			&parachain_config,
			telemetry_worker_handle,
			&mut task_manager,
			collator_options.clone(),
			None,
		)
		.await
		.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;

	let validator = parachain_config.role.is_authority();
	let prometheus_registry = parachain_config.prometheus_registry().cloned();

	let net_config =
		sc_network::config::FullNetworkConfiguration::<Block, _, N>::new(
			&parachain_config.network,
			prometheus_registry.clone(),
		);
	let metrics = N::register_notification_metrics(prometheus_registry.as_ref());

	let import_queue_service = import_queue.service();

	let (network, system_rpc_tx, tx_handler_controller, sync_service) =
		build_network(BuildNetworkParams {
			parachain_config: &parachain_config,
			net_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			para_id,
			spawn_handle: task_manager.spawn_handle(),
			spawn_essential_handle: task_manager.spawn_essential_handle(),
			relay_chain_interface: relay_chain_interface.clone(),
			import_queue,
			sybil_resistance_level: CollatorSybilResistance::Resistant,
			metrics,
		})
		.await?;

	let collator_peer_id = network.local_peer_id();

	let rpc_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();
		Box::new(move |_| {
			let deps = crate::rpc::FullDeps { client: client.clone(), pool: pool.clone() };
			crate::rpc::create_full(deps).map_err(Into::into)
		})
	};

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		rpc_builder,
		client: client.clone(),
		transaction_pool: transaction_pool.clone(),
		task_manager: &mut task_manager,
		config: parachain_config,
		keystore: keystore_container.keystore(),
		backend: backend.clone(),
		network: network.into(),
		system_rpc_tx,
		tx_handler_controller,
		telemetry: telemetry.as_mut(),
		sync_service: sync_service.clone(),
		tracing_execute_block: None,
	})?;

	let announce_block = {
		let sync_service = sync_service.clone();
		Arc::new(move |hash, data| sync_service.announce_block(hash, data))
	};

	let relay_chain_slot_duration = Duration::from_millis(6000);

	let overseer_handle = relay_chain_interface
		.overseer_handle()
		.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;

	start_relay_chain_tasks(StartRelayChainTasksParams {
		client: client.clone(),
		announce_block: announce_block.clone(),
		para_id,
		relay_chain_interface: relay_chain_interface.clone(),
		task_manager: &mut task_manager,
		da_recovery_profile: if validator {
			DARecoveryProfile::Collator
		} else {
			DARecoveryProfile::FullNode
		},
		import_queue: import_queue_service,
		relay_chain_slot_duration,
		recovery_handle: Box::new(overseer_handle.clone()),
		sync_service: sync_service.clone(),
		prometheus_registry: prometheus_registry.as_ref(),
	})?;

	if validator {
		start_consensus(
			client.clone(),
			block_import,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|t| t.handle()),
			&task_manager,
			relay_chain_interface.clone(),
			transaction_pool.clone(),
			keystore_container.keystore(),
			relay_chain_slot_duration,
			para_id,
			collator_key.expect("Collator key must be present for a validator node."),
			collator_peer_id,
			overseer_handle,
			announce_block,
		)?;
	}

	// ── PNS DNS server ───────────────────────────────────────────────────────
	if !no_dns {
		pns_ddns::start_dns_server::<
			FullClient,
			Block,
			u64,
			solochain_template_runtime::Balance,
			solochain_template_runtime::AccountId,
		>(client.clone(), dns_config);
	}

	Ok((task_manager, client))
}
