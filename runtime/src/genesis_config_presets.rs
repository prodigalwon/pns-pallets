// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{AccountId, BalancesConfig, RuntimeGenesisConfig, SudoConfig};
use alloc::{vec, vec::Vec};
use frame_support::build_struct_json_patch;
use serde_json::Value;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_genesis_builder::{self, PresetId};
use sp_keyring::Sr25519Keyring;

/// One token (12 decimal places).
const UNIT: u128 = 1_000_000_000_000;

// Returns the genesis config presets populated with given parameters.
fn testnet_genesis(
	initial_authorities: Vec<(AuraId, GrandpaId)>,
	endowed_accounts: Vec<AccountId>,
	root: AccountId,
) -> Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, 1u128 << 60))
				.collect::<Vec<_>>(),
		},
		aura: pallet_aura::GenesisConfig {
			authorities: initial_authorities.iter().map(|x| (x.0.clone())).collect::<Vec<_>>(),
		},
		grandpa: pallet_grandpa::GenesisConfig {
			authorities: initial_authorities.iter().map(|x| (x.1.clone(), 1)).collect::<Vec<_>>(),
		},
		sudo: SudoConfig { key: Some(root.clone()) },
		// ---- PNS bootstrap ----
		// Mint the native TLD base node NFT (class 0) to the sudo/root account.
		// This is the only action that cannot be done after the chain starts.
		pns_nft: pns_registrar::nft::GenesisConfig {
			tokens: vec![(
				root.clone(),           // class owner
				vec![],                 // class metadata
				(),                     // class data
				vec![(
					root.clone(),                      // token owner
					vec![],                            // token metadata
					pns_types::Record::default(),      // token data
					pns_types::NATIVE_BASENODE,        // token id (namehash of "dot" or "ksm")
				)],
			)],
		},
		// Registration and renewal fees indexed by label length (index 0 = 1-char, index 10 = 11+ chars).
		// base_prices: one-time registration fee (burned).
		// rent_prices: per-second holding fee (burned on renewal).
		// init_rate: exchange rate stored in state; used to convert fee units to native token.
		pns_price_oracle: pns_registrar::price_oracle::GenesisConfig {
			// Prices stored directly in planck (init_rate = 1, no exchange rate multiplication).
			// Index 0 = 1-char, index 1 = 2-char, ..., index 5+ = 6+ chars.
			base_prices: [
				1000 * UNIT, // 1 char
				100 * UNIT,  // 2 chars
				45 * UNIT,   // 3 chars
				25 * UNIT,   // 4 chars
				10 * UNIT,   // 5 chars
				UNIT / 2,    // 6 chars
				UNIT / 2,    // 7 chars
				UNIT / 2,    // 8 chars
				UNIT / 2,    // 9 chars
				UNIT / 2,    // 10 chars
				UNIT / 2,    // 11+ chars
			],
			rent_prices: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
			init_rate: 1,
		},
		// Set the official account (used for redeem-code signing and registry authority).
		pns_registry: pns_registrar::registry::GenesisConfig {
			official: Some(root),
			origin: vec![],
			operators: vec![],
			token_approvals: vec![],
		},
		pns_registrar: pns_registrar::registrar::GenesisConfig {
			infos: Default::default(),
			reserved_list: Default::default(),
		},
	})
}

/// Return the development genesis config.
pub fn development_config_genesis() -> Value {
	testnet_genesis(
		vec![(
			sp_keyring::Sr25519Keyring::Alice.public().into(),
			sp_keyring::Ed25519Keyring::Alice.public().into(),
		)],
		vec![
			Sr25519Keyring::Alice.to_account_id(),
			Sr25519Keyring::Bob.to_account_id(),
			Sr25519Keyring::AliceStash.to_account_id(),
			Sr25519Keyring::BobStash.to_account_id(),
		],
		sp_keyring::Sr25519Keyring::Alice.to_account_id(),
	)
}

/// Return the local genesis config preset.
pub fn local_config_genesis() -> Value {
	testnet_genesis(
		vec![
			(
				sp_keyring::Sr25519Keyring::Alice.public().into(),
				sp_keyring::Ed25519Keyring::Alice.public().into(),
			),
			(
				sp_keyring::Sr25519Keyring::Bob.public().into(),
				sp_keyring::Ed25519Keyring::Bob.public().into(),
			),
		],
		Sr25519Keyring::iter()
			.filter(|v| v != &Sr25519Keyring::One && v != &Sr25519Keyring::Two)
			.map(|v| v.to_account_id())
			.collect::<Vec<_>>(),
		Sr25519Keyring::Alice.to_account_id(),
	)
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let patch = match id.as_ref() {
		sp_genesis_builder::DEV_RUNTIME_PRESET => development_config_genesis(),
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => local_config_genesis(),
		_ => return None,
	};
	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
	]
}
