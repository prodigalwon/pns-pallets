use std::sync::Arc;

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use pns_types::{DomainHash, ListingInfo, NameRecord, RegistrarInfo};
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

    /// Get all DNS records for a namehash.
    #[method(name = "pns_lookup")]
    fn lookup(&self, node: DomainHash) -> RpcResult<Vec<(u32, Vec<u8>)>>;

    /// Get the active marketplace listing for a plain label (e.g. "alice"), or null if not listed.
    #[method(name = "pns_getListing")]
    fn get_listing(&self, name: String) -> RpcResult<Option<ListingInfo<AccountId, Balance, u64>>>;

    /// Compute the namehash for a plain label or dotted name (e.g. "alice" or "sub.alice").
    /// Returns null if the name is malformed. Use this to obtain the hash needed by
    /// `pns_getInfo` and `pns_lookup` without computing it client-side.
    #[method(name = "pns_nameToHash")]
    fn name_to_hash(&self, name: String) -> RpcResult<Option<DomainHash>>;

    /// Get all DNS records for a plain label or dotted name (e.g. "alice" or "sub.alice").
    /// Equivalent to calling `pns_nameToHash` then `pns_lookup` in one round-trip.
    #[method(name = "pns_lookupByName")]
    fn lookup_by_name(&self, name: String) -> RpcResult<Vec<(u32, Vec<u8>)>>;

    /// Return every registered name and its registrar info. Useful for indexers and explorers.
    #[method(name = "pns_all")]
    fn all(&self) -> RpcResult<Vec<(DomainHash, RegistrarInfo<u64, Balance>)>>;

    /// Check whether a name (by namehash) is currently useable — i.e. registered and not expired.
    /// Returns `true` if the name exists and is within its active registration period.
    #[method(name = "pns_isUseable")]
    fn is_useable(&self, node: DomainHash) -> RpcResult<bool>;
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

    fn lookup(&self, node: DomainHash) -> RpcResult<Vec<(u32, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let best = self.client.info().best_hash;
        let records = api.lookup(best, node).map_err(|e| jsonrpsee::types::ErrorObject::owned(
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

    fn name_to_hash(&self, name: String) -> RpcResult<Option<DomainHash>> {
        let api = self.client.runtime_api();
        let best = self.client.info().best_hash;
        api.name_to_hash(best, name.into_bytes())
            .map_err(|e| jsonrpsee::types::ErrorObject::owned(
                jsonrpsee::types::error::INTERNAL_ERROR_CODE,
                "Runtime error",
                Some(format!("{:?}", e)),
            ))
    }

    fn lookup_by_name(&self, name: String) -> RpcResult<Vec<(u32, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let best = self.client.info().best_hash;
        let records = api.lookup_by_name(best, name.into_bytes())
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

    fn is_useable(&self, node: DomainHash) -> RpcResult<bool> {
        let api = self.client.runtime_api();
        let best = self.client.info().best_hash;
        api.check_node_useable(best, node, &AccountId::from([0u8; 32])).map_err(|e| jsonrpsee::types::ErrorObject::owned(
            jsonrpsee::types::error::INTERNAL_ERROR_CODE,
            "Runtime error",
            Some(format!("{:?}", e)),
        ))
    }
}
