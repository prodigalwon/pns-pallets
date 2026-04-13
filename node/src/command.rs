use crate::{
	benchmarking::{inherent_benchmark_data, RemarkBuilder, TransferKeepAliveBuilder},
	chain_spec,
	cli::{Cli, RelayChainCli, Subcommand},
	service,
};
use cumulus_primitives_core::ParaId;
use frame_benchmarking_cli::{BenchmarkCmd, ExtrinsicFactory, SUBSTRATE_REFERENCE_HARDWARE};
use pns_ddns;
use sc_cli::SubstrateCli;
use sc_service::PartialComponents;
use solochain_template_runtime::{Block, EXISTENTIAL_DEPOSIT};
use sp_keyring::Sr25519Keyring;

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"PNS Node".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		env!("CARGO_PKG_DESCRIPTION").into()
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"support.anonymous.an".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(match id {
			"dev" => Box::new(chain_spec::development_chain_spec()?),
			"" | "local" => Box::new(chain_spec::local_chain_spec()?),
			"paseo" => Box::new(chain_spec::paseo_chain_spec(5169)?),
			path => Box::new(chain_spec::ChainSpec::from_json_file(
				std::path::PathBuf::from(path),
			)?),
		})
	}
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Key(cmd)) => cmd.run(&cli),
		#[allow(deprecated)]
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::ExportGenesisState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| {
				let PartialComponents { client, .. } = service::new_partial(&config)?;
				cmd.run(client)
			})
		},
		Some(Subcommand::ExportGenesisWasm(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(&*config.chain_spec))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, import_queue, .. } =
					service::new_partial(&config)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Some(Subcommand::ExportChainSpec(cmd)) => {
			let chain_spec = cli.load_spec(&cmd.chain)?;
			cmd.run(chain_spec)
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, .. } = service::new_partial(&config)?;
				Ok((cmd.run(client, config.database), task_manager))
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, .. } = service::new_partial(&config)?;
				Ok((cmd.run(client, config.chain_spec), task_manager))
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, import_queue, .. } =
					service::new_partial(&config)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| {
				let polkadot_cli = RelayChainCli::new(&config, cli.relay_chain_args.iter());
				let polkadot_config = sc_cli::SubstrateCli::create_configuration(
					&polkadot_cli,
					&polkadot_cli,
					config.tokio_handle.clone(),
				)
				.map_err(|e| format!("Relay chain argument error: {}", e))?;

				cmd.run(config, polkadot_config)
			})
		},
		Some(Subcommand::Revert(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents { client, task_manager, backend, .. } =
					service::new_partial(&config)?;
				Ok((cmd.run(client, backend, None), task_manager))
			})
		},
		Some(Subcommand::Benchmark(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			runner.sync_run(|config| {
				match cmd {
					BenchmarkCmd::Pallet(cmd) => {
						if !cfg!(feature = "runtime-benchmarks") {
							return Err(
								"Runtime benchmarking wasn't enabled when building the node. \
							You can enable it with `--features runtime-benchmarks`."
									.into(),
							);
						}

						cmd.run_with_spec::<sp_runtime::traits::HashingFor<Block>, ()>(Some(
							config.chain_spec,
						))
					},
					BenchmarkCmd::Block(cmd) => {
						let PartialComponents { client, .. } = service::new_partial(&config)?;
						cmd.run(client)
					},
					#[cfg(not(feature = "runtime-benchmarks"))]
					BenchmarkCmd::Storage(_) => Err(
						"Storage benchmarking can be enabled with `--features runtime-benchmarks`."
							.into(),
					),
					#[cfg(feature = "runtime-benchmarks")]
					BenchmarkCmd::Storage(cmd) => {
						let PartialComponents { client, backend, .. } =
							service::new_partial(&config)?;
						let db = backend.expose_db();
						let storage = backend.expose_storage();
						let shared_cache = backend.expose_shared_trie_cache();

						cmd.run(config, client, db, storage, shared_cache)
					},
					BenchmarkCmd::Overhead(cmd) => {
						let PartialComponents { client, .. } = service::new_partial(&config)?;
						let ext_builder = RemarkBuilder::new(client.clone());

						cmd.run(
							config.chain_spec.name().into(),
							client,
							inherent_benchmark_data()?,
							Vec::new(),
							&ext_builder,
							false,
						)
					},
					BenchmarkCmd::Extrinsic(cmd) => {
						let PartialComponents { client, .. } = service::new_partial(&config)?;
						let ext_factory = ExtrinsicFactory(vec![
							Box::new(RemarkBuilder::new(client.clone())),
							Box::new(TransferKeepAliveBuilder::new(
								client.clone(),
								Sr25519Keyring::Alice.to_account_id(),
								EXISTENTIAL_DEPOSIT,
							)),
						]);

						cmd.run(client, inherent_benchmark_data()?, Vec::new(), &ext_factory)
					},
					BenchmarkCmd::Machine(cmd) => {
						cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone())
					},
				}
			})
		},
		Some(Subcommand::ChainInfo(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run::<Block>(&config))
		},
		None => {
			let dns_config = pns_ddns::DnsConfig {
				port: cli.dns_port,
				workers: cli.dns_workers,
				cores: cli.dns_cores.clone(),
				min_response_ms: cli.dns_min_response_ms,
			};
			let no_dns = cli.no_dns;
			let runner = cli.create_runner(&cli.run.base)?;

			let collator_options = cli.run.collator_options();

			runner.run_node_until_exit(|config| async move {
				let para_id =
					chain_spec::Extensions::try_get(&*config.chain_spec)
						.map(|e| e.para_id)
						.ok_or("Could not find parachain extension in chain-spec. \
							Use a chain spec with relay_chain and para_id fields, \
							or generate one with `export-chain-spec`.")?;

				let polkadot_cli =
					RelayChainCli::new(&config, cli.relay_chain_args.iter());

				let para_id = ParaId::from(para_id);

				let polkadot_config = sc_cli::SubstrateCli::create_configuration(
					&polkadot_cli,
					&polkadot_cli,
					config.tokio_handle.clone(),
				)
				.map_err(|e| format!("Relay chain argument error: {}", e))?;

				match config.network.network_backend {
					sc_network::config::NetworkBackendType::Libp2p =>
						service::start_parachain_node::<
							sc_network::NetworkWorker<
								solochain_template_runtime::opaque::Block,
								<solochain_template_runtime::opaque::Block as sp_runtime::traits::Block>::Hash,
							>,
						>(config, polkadot_config, collator_options, para_id, no_dns, dns_config)
						.await
						.map(|r| r.0)
						.map_err(sc_cli::Error::Service),
					sc_network::config::NetworkBackendType::Litep2p =>
						service::start_parachain_node::<sc_network::Litep2pNetworkBackend>(
							config,
							polkadot_config,
							collator_options,
							para_id,
							no_dns,
							dns_config,
						)
						.await
						.map(|r| r.0)
						.map_err(sc_cli::Error::Service),
				}
			})
		},
	}
}
