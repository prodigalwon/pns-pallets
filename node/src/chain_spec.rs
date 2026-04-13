use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use solochain_template_runtime::WASM_BINARY;

/// Extension fields embedded in the chain spec JSON.
/// The node reads these to know which relay chain to connect to and its own para ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecExtension, ChainSpecGroup)]
#[serde(deny_unknown_fields)]
pub struct Extensions {
	/// The relay chain identifier (e.g. "paseo", "rococo-local").
	pub relay_chain: String,
	/// The parachain ID.
	pub para_id: u32,
}

impl Extensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

/// Specialized `ChainSpec` for this parachain.
pub type ChainSpec = sc_service::GenericChainSpec<Extensions>;

pub fn development_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		Extensions { relay_chain: "rococo-local".to_string(), para_id: 1000 },
	)
	.with_name("Development")
	.with_id("dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.build())
}

pub fn local_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		Extensions { relay_chain: "rococo-local".to_string(), para_id: 1000 },
	)
	.with_name("Local Testnet")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.build())
}

/// Paseo chain spec — fill in your actual ParaID below.
pub fn paseo_chain_spec(para_id: u32) -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Paseo wasm not available".to_string())?,
		Extensions { relay_chain: "paseo".to_string(), para_id },
	)
	.with_name("PNS Paseo")
	.with_id("pns_paseo")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.build())
}
