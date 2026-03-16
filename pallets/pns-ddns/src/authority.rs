use std::{marker::PhantomData, net::Ipv4Addr, net::Ipv6Addr, str::FromStr, sync::Arc};

use pns_runtime_api::PnsStorageApi as _;

use async_trait::async_trait;
use hickory_proto::{
    op::ResponseCode,
    rr::{
        DNSClass, LowerName, Name, RData, Record, RecordType,
        rdata::{A, AAAA, CNAME, MX, NS, SOA, SRV, TXT},
    },
};
use hickory_server::authority::{
    Authority, LookupControlFlow, LookupError, LookupObject, LookupOptions, MessageRequest,
    UpdateResult, ZoneType,
};
use hickory_server::server::RequestInfo;
use polkadot_sdk::sp_api::ProvideRuntimeApi;
use polkadot_sdk::sp_blockchain::HeaderBackend;
use polkadot_sdk::sp_runtime::traits::Block as BlockT;
use pns_types::ddns::codec_type::RecordType as PnsRecordType;
use tracing::{debug, warn};

use crate::MAX_CNAME_DEPTH;

/// DNS records backed by PNS on-chain storage.
///
/// Serves the `.dot` zone. Standard DNS types stored on-chain are served to
/// resolvers. All Polkadot-specific record types (SS58, VALIDATOR, PARA,
/// PROXY, PUBKEY*, AVATAR, CONTRACT, CONTENT, IPFS, ORIGIN) are NOT exposed
/// via DNS — they are only accessible through the JSON-RPC API.
///
/// The RPC record type (65281) IS served: if it contains a valid IPv4/IPv6
/// address it maps to A/AAAA; otherwise it maps to a CNAME.
pub struct BlockChainAuthority<C, Block, Dur, Bal, Acc> {
    client: Arc<C>,
    origin: LowerName,
    _phantom: PhantomData<(Block, Dur, Bal, Acc)>,
}

impl<C, Block, Dur, Bal, Acc> BlockChainAuthority<C, Block, Dur, Bal, Acc>
where
    Block: BlockT,
{
    pub fn new(client: Arc<C>) -> Self {
        let origin = LowerName::from(Name::from_str("dot.").expect("valid zone origin"));
        Self { client, origin, _phantom: PhantomData }
    }
}

// ── Lookup result type ───────────────────────────────────────────────────────

pub struct PnsLookup {
    records: Vec<Record>,
    additionals: Option<Vec<Record>>,
}

impl PnsLookup {
    fn empty() -> Self {
        Self { records: vec![], additionals: None }
    }
    fn with_records(records: Vec<Record>) -> Self {
        Self { records, additionals: None }
    }
}

impl LookupObject for PnsLookup {
    fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = &'a Record> + Send + 'a> {
        Box::new(self.records.iter())
    }
    fn take_additionals(&mut self) -> Option<Box<dyn LookupObject>> {
        self.additionals.take().map(|add| {
            Box::new(PnsLookup { records: add, additionals: None }) as Box<dyn LookupObject>
        })
    }
}

// ── Helper fns ───────────────────────────────────────────────────────────────

/// Returns true for PNS-specific record types that must NOT be served via DNS.
/// RPC (65281) is intentionally NOT in this list — it is mapped to A/AAAA/CNAME.
fn is_polkadot_only(rt: &PnsRecordType) -> bool {
    matches!(
        rt,
        PnsRecordType::SS58
            | PnsRecordType::VALIDATOR
            | PnsRecordType::PARA
            | PnsRecordType::PROXY
            | PnsRecordType::PUBKEY1
            | PnsRecordType::PUBKEY2
            | PnsRecordType::PUBKEY3
            | PnsRecordType::AVATAR
            | PnsRecordType::CONTRACT
            | PnsRecordType::CONTENT
            | PnsRecordType::IPFS
            | PnsRecordType::ORIGIN
    )
}

/// Extract the plain label from a fully-qualified DNS name relative to `.dot.`
/// e.g. `alice.dot.` → `"alice"`,  `sub.alice.dot.` → `"sub.alice"`, `dot.` → `""`
fn extract_label(name: &LowerName, origin: &LowerName) -> Option<String> {
    if name == origin {
        return Some(String::new()); // zone apex
    }
    let name_s = name.to_string(); // e.g. "alice.dot."
    let origin_s = origin.to_string(); // "dot."
    // Strip the trailing ".{origin}" suffix
    let suffix = format!(".{}", origin_s);
    name_s.strip_suffix(&suffix).map(|s| s.to_string())
}

/// Synthesize the SOA record for the `.dot.` zone using the current block number
/// as the serial.
fn synthesize_soa(origin: &LowerName, serial: u32) -> Record {
    let mname = Name::from_str("ns1.dot.").unwrap();
    let rname = Name::from_str("hostmaster.dot.").unwrap();
    let soa = SOA::new(mname, rname, serial, 3600, 900, 86400, 300);
    Record::from_rdata(origin.into(), 300, RData::SOA(soa))
}

/// Synthesize the NS record for the `.dot.` zone.
fn synthesize_ns(origin: &LowerName) -> Record {
    let ns_name = Name::from_str("ns1.dot.").unwrap();
    Record::from_rdata(origin.into(), 300, RData::NS(NS(ns_name)))
}

/// Try to convert a PNS raw record `(type, bytes)` into a DNS `Record` for
/// the given `owner_name`. Returns `None` for types that are not DNS-serveable
/// or for malformed byte payloads.
fn pns_record_to_dns(
    owner_name: &Name,
    pns_type: &PnsRecordType,
    data: &[u8],
    queried_type: RecordType,
) -> Option<Record> {
    if is_polkadot_only(pns_type) {
        return None;
    }

    let rdata: RData = match pns_type {
        // ── Standard DNS types ────────────────────────────────────────────
        PnsRecordType::A => {
            if data.len() != 4 {
                return None;
            }
            RData::A(A(Ipv4Addr::from([data[0], data[1], data[2], data[3]])))
        }
        PnsRecordType::AAAA => {
            if data.len() != 16 {
                return None;
            }
            let mut arr = [0u8; 16];
            arr.copy_from_slice(data);
            RData::AAAA(AAAA(Ipv6Addr::from(arr)))
        }
        PnsRecordType::CNAME => {
            let target = std::str::from_utf8(data).ok()?;
            let name = Name::from_str(target).ok()?;
            RData::CNAME(CNAME(name))
        }
        PnsRecordType::TXT => {
            RData::TXT(TXT::new(vec![String::from_utf8_lossy(data).into_owned()]))
        }
        PnsRecordType::MX => {
            if data.len() < 3 {
                return None;
            }
            let preference = u16::from_be_bytes([data[0], data[1]]);
            let exchange_str = std::str::from_utf8(&data[2..]).ok()?;
            let exchange = Name::from_str(exchange_str).ok()?;
            RData::MX(MX::new(preference, exchange))
        }
        PnsRecordType::NS => {
            let ns_str = std::str::from_utf8(data).ok()?;
            let ns_name = Name::from_str(ns_str).ok()?;
            RData::NS(NS(ns_name))
        }
        PnsRecordType::SRV => {
            if data.len() < 7 {
                return None;
            }
            let priority = u16::from_be_bytes([data[0], data[1]]);
            let weight   = u16::from_be_bytes([data[2], data[3]]);
            let port     = u16::from_be_bytes([data[4], data[5]]);
            let target_str = std::str::from_utf8(&data[6..]).ok()?;
            let target = Name::from_str(target_str).ok()?;
            RData::SRV(SRV::new(priority, weight, port, target))
        }
        // ── RPC — served as A/AAAA/CNAME based on content ────────────────
        PnsRecordType::RPC => {
            let text = std::str::from_utf8(data).ok()?;
            if let Ok(ip4) = text.parse::<Ipv4Addr>() {
                // Only serve as A if the query was for A or ANY
                if !matches!(queried_type, RecordType::A | RecordType::ANY) {
                    return None;
                }
                RData::A(A(ip4))
            } else if let Ok(ip6) = text.parse::<Ipv6Addr>() {
                if !matches!(queried_type, RecordType::AAAA | RecordType::ANY) {
                    return None;
                }
                RData::AAAA(AAAA(ip6))
            } else if let Ok(name) = Name::from_str(text) {
                if !matches!(queried_type, RecordType::CNAME | RecordType::ANY) {
                    return None;
                }
                RData::CNAME(CNAME(name))
            } else {
                return None;
            }
        }
        // ── Unknown standard DNS type stored as raw bytes ─────────────────
        _ => {
            // Convert via hickory type mapping to get the numeric code
            let htype: RecordType = RecordType::from(pns_type.clone());
            // Only serve if the stored type matches what was queried (or ANY)
            if htype != queried_type && queried_type != RecordType::ANY {
                return None;
            }
            // Serve as Unknown RData
            RData::Unknown {
                code: htype,
                rdata: hickory_proto::rr::rdata::NULL::with(data.to_vec()),
            }
        }
    };

    Some(
        Record::from_rdata(owner_name.clone(), 300, rdata)
            .set_dns_class(DNSClass::IN)
            .clone(),
    )
}

// ── Authority impl ───────────────────────────────────────────────────────────

#[async_trait]
impl<C, Block, Dur, Bal, Acc> Authority for BlockChainAuthority<C, Block, Dur, Bal, Acc>
where
    C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
    C::Api: pns_runtime_api::PnsStorageApi<Block, Dur, Bal, Acc>,
    Block: BlockT + 'static,
    Dur: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
    Bal: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
    Acc: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
{
    type Lookup = PnsLookup;

    fn zone_type(&self) -> ZoneType {
        ZoneType::Primary
    }

    fn is_axfr_allowed(&self) -> bool {
        false
    }

    async fn update(&self, _update: &MessageRequest) -> UpdateResult<bool> {
        Err(ResponseCode::NotImp)
    }

    fn origin(&self) -> &LowerName {
        &self.origin
    }

    async fn lookup(
        &self,
        name: &LowerName,
        rtype: RecordType,
        _opts: LookupOptions,
    ) -> LookupControlFlow<Self::Lookup> {
        use LookupControlFlow::Continue;

        let at = self.client.info().best_hash;
        let block_number = self.client.info().best_number;
        // Use low 32 bits of block number as SOA serial.
        let serial = {
            use polkadot_sdk::sp_runtime::traits::UniqueSaturatedInto;
            let n: u64 = block_number.unique_saturated_into();
            n as u32
        };

        // Zone apex — serve SOA and NS
        if name == &self.origin {
            let records = match rtype {
                RecordType::SOA | RecordType::ANY => {
                    vec![synthesize_soa(&self.origin, serial)]
                }
                RecordType::NS | RecordType::ANY => {
                    vec![synthesize_ns(&self.origin)]
                }
                _ => vec![],
            };
            return if records.is_empty() {
                Continue(Err(LookupError::NameExists))
            } else {
                Continue(Ok(PnsLookup::with_records(records)))
            };
        }

        // Extract the PNS label
        let label = match extract_label(name, &self.origin) {
            Some(l) => l,
            None => return Continue(Err(LookupError::from(ResponseCode::NXDomain))),
        };

        // SOA always served from apex only
        if rtype == RecordType::SOA {
            return Continue(Err(LookupError::NameExists));
        }

        // Call the runtime API
        let api = self.client.runtime_api();
        let raw_records: Vec<(pns_types::ddns::codec_type::RecordType, Vec<u8>)> =
            match api.lookup_by_name(at, label.into_bytes()) {
            Ok(r) => r,
            Err(e) => {
                warn!("BlockChainAuthority: lookup_by_name error: {:?}", e);
                return Continue(Err(LookupError::from(ResponseCode::ServFail)));
            }
        };

        if raw_records.is_empty() {
            return Continue(Err(LookupError::from(ResponseCode::NXDomain)));
        }

        let owner_name: Name = name.into();
        let mut dns_records: Vec<Record> = Vec::new();
        let mut cname_depth = 0usize;

        for (pns_type, data) in &raw_records {
            // Enforce CNAME depth cap
            if matches!(pns_type, PnsRecordType::CNAME) {
                cname_depth += 1;
                if cname_depth > MAX_CNAME_DEPTH {
                    debug!("CNAME depth cap reached for {}", name);
                    break;
                }
            }
            if let Some(rec) = pns_record_to_dns(&owner_name, pns_type, data, rtype) {
                dns_records.push(rec);
            }
        }

        if dns_records.is_empty() {
            // Name exists but no records of the queried type
            Continue(Err(LookupError::NameExists))
        } else {
            Continue(Ok(PnsLookup::with_records(dns_records)))
        }
    }

    async fn search(
        &self,
        request_info: RequestInfo<'_>,
        lookup_options: LookupOptions,
    ) -> LookupControlFlow<Self::Lookup> {
        let name = request_info.query.name();
        let rtype = request_info.query.query_type();
        self.lookup(name, rtype, lookup_options).await
    }

    async fn get_nsec_records(
        &self,
        _name: &LowerName,
        _opts: LookupOptions,
    ) -> LookupControlFlow<Self::Lookup> {
        // No DNSSEC support
        LookupControlFlow::Skip
    }
}
