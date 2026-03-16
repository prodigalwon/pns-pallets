#[derive(Debug, clap::Parser)]
pub struct Cli {
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[clap(flatten)]
	pub run: sc_cli::RunCmd,

	/// Disable the built-in UDP DNS server.
	/// When disabled the node operates without serving `.dot` DNS queries.
	/// Use this flag if you run a separate DNS process or do not need DNS resolution.
	#[clap(long = "no-dns")]
	pub no_dns: bool,

	/// UDP port for the DNS server (default: 53).
	/// Port 53 requires root or CAP_NET_BIND_SERVICE.
	/// Use an unprivileged port (e.g. 5353) and proxy with dnsdist for production.
	#[clap(long = "dns-port", default_value = "53")]
	pub dns_port: u16,

	/// Number of worker threads in the dedicated DNS tokio runtime (default: 2).
	/// DNS runs on its own runtime isolated from consensus and networking.
	#[clap(long = "dns-workers", default_value = "2")]
	pub dns_workers: usize,

	/// Minimum response time in milliseconds enforced by the interval-based
	/// response queue (default: 5). Prevents timing-based name enumeration attacks.
	#[clap(long = "dns-min-response-ms", default_value = "5")]
	pub dns_min_response_ms: u64,

	/// Comma-separated list of CPU core IDs to pin DNS worker threads to.
	/// If omitted, the highest-numbered cores are selected automatically.
	/// If the system has fewer than 3 cores, affinity is skipped entirely and
	/// a warning is logged that DNS and consensus are sharing CPU resources.
	#[clap(long = "dns-cores", value_delimiter = ',')]
	pub dns_cores: Option<Vec<usize>>,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Subcommand {
	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification.
	/// DEPRECATED: `build-spec` command will be removed after 1/04/2026. Use `export-chain-spec`
	/// command instead.
	#[deprecated(
		note = "build-spec command will be removed after 1/04/2026. Use export-chain-spec command instead"
	)]
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Export the chain specification.
	ExportChainSpec(sc_cli::ExportChainSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Sub-commands concerned with benchmarking.
	#[command(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}
