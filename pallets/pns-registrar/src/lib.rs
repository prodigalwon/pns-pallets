#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
extern crate alloc;

pub mod migration;
pub mod nft;
pub mod origin;
pub mod price_oracle;
pub mod registrar;
pub mod registry;
pub mod traits;
