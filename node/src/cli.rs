use sc_cli::SubstrateCli;

#[derive(Debug, clap::Parser)]
pub struct Cli {
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[clap(flatten)]
	pub run: cumulus_client_cli::RunCmd,

	/// Disable the built-in UDP DNS server.
	#[clap(long = "no-dns")]
	pub no_dns: bool,

	/// UDP port for the DNS server (default: 53).
	#[clap(long = "dns-port", default_value = "53")]
	pub dns_port: u16,

	/// Number of worker threads in the dedicated DNS tokio runtime (default: 2).
	#[clap(long = "dns-workers", default_value = "2")]
	pub dns_workers: usize,

	/// Minimum response time in milliseconds enforced by the DNS response queue (default: 5).
	#[clap(long = "dns-min-response-ms", default_value = "5")]
	pub dns_min_response_ms: u64,

	/// Comma-separated list of CPU core IDs to pin DNS worker threads to.
	#[clap(long = "dns-cores", value_delimiter = ',')]
	pub dns_cores: Option<Vec<usize>>,

	/// Relay chain arguments passed after `--` to the embedded relay chain node.
	/// For Paseo use `--relay-chain-rpc-urls` instead.
	#[arg(raw = true)]
	pub relay_chain_args: Vec<String>,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Subcommand {
	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification (deprecated, use export-chain-spec).
	#[deprecated(
		note = "build-spec will be removed after 1/04/2026. Use export-chain-spec instead"
	)]
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Export the chain specification.
	ExportChainSpec(sc_cli::ExportChainSpecCmd),

	/// Export the genesis state of the parachain (needed for registration on the relay chain).
	ExportGenesisState(cumulus_client_cli::ExportGenesisHeadCommand),

	/// Export the genesis WASM blob of the parachain runtime (needed for registration).
	ExportGenesisWasm(cumulus_client_cli::ExportGenesisWasmCommand),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain (also removes relay chain data).
	PurgeChain(cumulus_client_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Sub-commands concerned with benchmarking.
	#[command(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}

/// Thin wrapper around the relay chain node arguments.
/// When `--relay-chain-rpc-urls` is given, no embedded relay chain node is started.
#[derive(Debug)]
pub struct RelayChainCli {
	/// The sc-cli RunCmd that drives the embedded relay chain node.
	pub base: sc_cli::RunCmd,
	/// Override for the relay chain base path (defaults to `<parachain-base>/polkadot`).
	pub base_path: Option<std::path::PathBuf>,
}

impl RelayChainCli {
	/// Build from the parachain config and the trailing relay chain args.
	pub fn new<'a>(
		para_config: &sc_service::Configuration,
		relay_chain_args: impl Iterator<Item = &'a String>,
	) -> Self {
		let base_path = para_config.base_path.path().join("polkadot");
		Self {
			base: clap::Parser::parse_from(
				std::iter::once("polkadot").chain(relay_chain_args.map(|s| s.as_str())),
			),
			base_path: Some(base_path),
		}
	}
}

impl SubstrateCli for RelayChainCli {
	fn impl_name() -> String { "Polkadot".into() }
	fn impl_version() -> String { "0".into() }
	fn description() -> String { "Polkadot relay chain (embedded)".into() }
	fn author() -> String { "Parity Technologies".into() }
	fn support_url() -> String { "https://github.com/paritytech/polkadot-sdk".into() }
	fn copyright_start_year() -> i32 { 2017 }

	fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		// Attempt to load from a JSON file path; fall back to well-known chain IDs
		// handled by polkadot-service when running an embedded relay-chain node.
		// When --relay-chain-rpc-urls is given, no embedded node is started and
		// this function is not called on the relay-chain path.
		sc_service::GenericChainSpec::<crate::chain_spec::Extensions>::from_json_file(id.into())
			.map(|s| Box::new(s) as Box<_>)
			.map_err(|e| format!("{}", e))
	}
}

impl sc_cli::DefaultConfigurationValues for RelayChainCli {
	fn p2p_listen_port() -> u16 { 30334 }
	fn rpc_listen_port() -> u16 { 9945 }
	fn prometheus_listen_port() -> u16 { 9616 }
}

impl sc_cli::CliConfiguration<Self> for RelayChainCli {
	fn shared_params(&self) -> &sc_cli::SharedParams { &self.base.shared_params }
	fn import_params(&self) -> Option<&sc_cli::ImportParams> { Some(&self.base.import_params) }
	fn network_params(&self) -> Option<&sc_cli::NetworkParams> { Some(&self.base.network_params) }
	fn keystore_params(&self) -> Option<&sc_cli::KeystoreParams> { Some(&self.base.keystore_params) }

	fn base_path(&self) -> sc_cli::Result<Option<sc_service::BasePath>> {
		Ok(self.base_path.clone().map(Into::into))
	}

	fn rpc_addr(&self, default_listen_port: u16) -> sc_cli::Result<Option<Vec<sc_cli::RpcEndpoint>>> {
		self.base.rpc_addr(default_listen_port)
	}

	fn prometheus_config(
		&self,
		default_listen_port: u16,
		chain_spec: &Box<dyn sc_service::ChainSpec>,
	) -> sc_cli::Result<Option<sc_service::config::PrometheusConfig>> {
		self.base.prometheus_config(default_listen_port, chain_spec)
	}

	fn init<F>(
		&self,
		_support_url: &String,
		_impl_version: &String,
		_logger_hook: F,
	) -> sc_cli::Result<()>
	where
		F: FnOnce(&mut sc_cli::LoggerBuilder),
	{
		unreachable!("CliConfiguration::init is not called for RelayChainCli")
	}

	fn chain_id(&self, is_dev: bool) -> sc_cli::Result<String> {
		<sc_cli::RunCmd as sc_cli::CliConfiguration>::chain_id(&self.base, is_dev)
	}

	fn role(&self, is_dev: bool) -> sc_cli::Result<sc_service::Role> {
		<sc_cli::RunCmd as sc_cli::CliConfiguration>::role(&self.base, is_dev)
	}

	fn transaction_pool(
		&self,
		is_dev: bool,
	) -> sc_cli::Result<sc_service::config::TransactionPoolOptions> {
		<sc_cli::RunCmd as sc_cli::CliConfiguration>::transaction_pool(&self.base, is_dev)
	}
}
