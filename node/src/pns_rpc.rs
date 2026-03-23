use std::sync::Arc;

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use pns_types::{AccountDashboard, DomainHash, ListingInfo, NameRecord, RegistrarInfo};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use solochain_template_runtime::{AccountId, Balance, opaque::Block};
use pns_runtime_api::PnsStorageApi as PnsRuntimeApi;

#[rpc(server)]
pub trait PnsStorageApi {
    /// Get registrar info by namehash, including the current owner.
    #[method(name = "pns_getInfo")]
    fn get_info(&self, node: DomainHash) -> RpcResult<Option<NameRecord<AccountId, u64, Balance>>>;

    /// Resolve a plain label (e.g. "alice") to its full name record including owner.
    #[method(name = "pns_resolveName")]
    fn resolve_name(&self, name: String) -> RpcResult<Option<NameRecord<AccountId, u64, Balance>>>;

    /// Get DNS records for a namehash, filtered to the requested record type codes.
    /// `record_types` is a list of IANA type numbers (e.g. 1 = A, 28 = AAAA, 65280 = SS58).
    /// SS58 (65280) is always included in the response when present, regardless of the filter.
    #[method(name = "pns_lookup")]
    fn lookup(&self, node: DomainHash, record_types: Vec<u32>) -> RpcResult<Vec<(u32, Vec<u8>)>>;

    /// Get the active marketplace listing for a plain label (e.g. "alice"), or null if not listed.
    #[method(name = "pns_getListing")]
    fn get_listing(&self, name: String) -> RpcResult<Option<ListingInfo<AccountId, Balance, u64>>>;

    /// Get DNS records for a plain label or dotted name, filtered to the requested record type codes.
    /// `record_types` is a list of IANA type numbers (e.g. 1 = A, 28 = AAAA, 65280 = SS58).
    #[method(name = "pns_lookupByName")]
    fn lookup_by_name(&self, name: String, record_types: Vec<u32>) -> RpcResult<Vec<(u32, Vec<u8>)>>;

    /// Return every registered name and its registrar info. Useful for indexers and explorers.
    #[method(name = "pns_all")]
    fn all(&self) -> RpcResult<Vec<(DomainHash, RegistrarInfo<u64, Balance>)>>;

    /// Return a full name portfolio summary for an account in a single call.
    /// Includes: primary name hash, active subname hashes, pending subdomain offer hashes,
    /// and pending top-level name gift offer hashes.
    #[method(name = "pns_accountDashboard")]
    fn account_dashboard(&self, account: AccountId) -> RpcResult<AccountDashboard>;
}

pub struct PnsRpc<C> {
    client: Arc<C>,
}

impl<C> PnsRpc<C> {
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }
}

impl<C> PnsStorageApiServer for PnsRpc<C>
where
    C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
    C::Api: pns_runtime_api::PnsStorageApi<Block, u64, Balance, AccountId>,
{
    fn get_info(&self, node: DomainHash) -> RpcResult<Option<NameRecord<AccountId, u64, Balance>>> {
        let chain = self.client.info();
        let api = self.client.runtime_api();
        api.get_info(chain.best_hash, node)
            .map_err(|e| jsonrpsee::types::ErrorObject::owned(
                jsonrpsee::types::error::INTERNAL_ERROR_CODE,
                "Runtime error",
                Some(format!("{:?}", e)),
            ))
            .map(|opt: Option<NameRecord<AccountId, u64, Balance>>| opt.map(|mut r| {
                r.read_block_hash = chain.best_hash;
                r.read_block_number = chain.best_number;
                r
            }))
    }

    fn resolve_name(&self, name: String) -> RpcResult<Option<NameRecord<AccountId, u64, Balance>>> {
        let chain = self.client.info();
        let api = self.client.runtime_api();
        api.resolve_name(chain.best_hash, name.into_bytes())
            .map_err(|e| jsonrpsee::types::ErrorObject::owned(
                jsonrpsee::types::error::INTERNAL_ERROR_CODE,
                "Runtime error",
                Some(format!("{:?}", e)),
            ))
            .map(|opt: Option<NameRecord<AccountId, u64, Balance>>| opt.map(|mut r| {
                r.read_block_hash = chain.best_hash;
                r.read_block_number = chain.best_number;
                r
            }))
    }

    fn lookup(&self, node: DomainHash, record_types: Vec<u32>) -> RpcResult<Vec<(u32, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let best = self.client.info().best_hash;
        let pns_types = record_types.into_iter()
            .map(|code| hickory_proto::rr::RecordType::from(code as u16).into())
            .collect();
        let records = api.lookup(best, node, pns_types).map_err(|e| jsonrpsee::types::ErrorObject::owned(
            jsonrpsee::types::error::INTERNAL_ERROR_CODE,
            "Runtime error",
            Some(format!("{:?}", e)),
        ))?;
        Ok(records.into_iter().map(|(rt, data)| {
            let code = u16::from(hickory_proto::rr::RecordType::from(rt)) as u32;
            (code, data)
        }).collect())
    }

    fn get_listing(&self, name: String) -> RpcResult<Option<ListingInfo<AccountId, Balance, u64>>> {
        let chain = self.client.info();
        let api = self.client.runtime_api();
        api.get_listing(chain.best_hash, name.into_bytes())
            .map_err(|e| jsonrpsee::types::ErrorObject::owned(
                jsonrpsee::types::error::INTERNAL_ERROR_CODE,
                "Runtime error",
                Some(format!("{:?}", e)),
            ))
            .map(|opt: Option<ListingInfo<AccountId, Balance, u64>>| opt.map(|mut r| {
                r.read_block_hash = chain.best_hash;
                r.read_block_number = chain.best_number;
                r
            }))
    }

    fn lookup_by_name(&self, name: String, record_types: Vec<u32>) -> RpcResult<Vec<(u32, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let best = self.client.info().best_hash;
        let pns_types = record_types.into_iter()
            .map(|code| hickory_proto::rr::RecordType::from(code as u16).into())
            .collect();
        let records = api.lookup_by_name(best, name.into_bytes(), pns_types)
            .map_err(|e| jsonrpsee::types::ErrorObject::owned(
                jsonrpsee::types::error::INTERNAL_ERROR_CODE,
                "Runtime error",
                Some(format!("{:?}", e)),
            ))?;
        Ok(records.into_iter().map(|(rt, data)| {
            let code = u16::from(hickory_proto::rr::RecordType::from(rt)) as u32;
            (code, data)
        }).collect())
    }

    fn all(&self) -> RpcResult<Vec<(DomainHash, RegistrarInfo<u64, Balance>)>> {
        let api = self.client.runtime_api();
        let best = self.client.info().best_hash;
        api.all(best).map_err(|e| jsonrpsee::types::ErrorObject::owned(
            jsonrpsee::types::error::INTERNAL_ERROR_CODE,
            "Runtime error",
            Some(format!("{:?}", e)),
        ))
    }

    fn account_dashboard(&self, account: AccountId) -> RpcResult<AccountDashboard> {
        let api = self.client.runtime_api();
        let best = self.client.info().best_hash;
        api.account_dashboard(best, account)
            .map_err(|e| jsonrpsee::types::ErrorObject::owned(
                jsonrpsee::types::error::INTERNAL_ERROR_CODE,
                "Runtime error",
                Some(format!("{:?}", e)),
            ))
    }
}
