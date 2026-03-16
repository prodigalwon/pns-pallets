use core::marker::PhantomData;
use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use pns_registrar::{registrar::BalanceOf, traits::Label};
use pns_runtime_api::PnsStorageApi;
use pns_types::DomainHash;
use polkadot_sdk::sc_client_api::backend::Backend as BackendT;
use polkadot_sdk::sp_api::ProvideRuntimeApi;
use polkadot_sdk::sp_block_builder::BlockBuilder;
use polkadot_sdk::sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use polkadot_sdk::sp_runtime::traits::Block as BlockT;
use tracing::error;

pub mod authority;
pub mod dns_server;
pub mod metrics;

pub use dns_server::start_dns_server;

/// Maximum CNAME chain depth enforced by BlockChainAuthority during record construction.
pub const MAX_CNAME_DEPTH: usize = 8;

/// Configuration for the dedicated DNS server runtime.
#[derive(Clone, Debug)]
pub struct DnsConfig {
    /// UDP port for the DNS listener (default 53).
    pub port: u16,
    /// Number of worker threads in the dedicated DNS tokio runtime (default 2).
    pub workers: usize,
    /// Optional list of CPU core IDs to pin DNS worker threads to.
    /// If None, the highest-numbered cores are selected automatically.
    pub cores: Option<Vec<usize>>,
    /// Minimum response time in milliseconds enforced by the interval-based
    /// response queue (default 5).
    pub min_response_ms: u64,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self { port: 53, workers: 2, cores: None, min_response_ms: 5 }
    }
}

// ── Axum HTTP REST server (unchanged) ────────────────────────────────────────

pub struct ServerDeps<Client, Backend, Block, Config>
where
    Block: BlockT,
    Backend: BackendT<Block>,
{
    pub client: Arc<Client>,
    pub backend: Arc<Backend>,
    _block: PhantomData<(Block, Config)>,
}

impl<Client, Backend, Block, Config> Clone for ServerDeps<Client, Backend, Block, Config>
where
    Block: BlockT,
    Backend: BackendT<Block>,
{
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            backend: self.backend.clone(),
            _block: PhantomData,
        }
    }
}

unsafe impl<Client, Backend, Block, Config> Send for ServerDeps<Client, Backend, Block, Config>
where
    Client: Send,
    Block: BlockT,
    Backend: BackendT<Block>,
{
}

unsafe impl<Client, Backend, Block, Config> Sync for ServerDeps<Client, Backend, Block, Config>
where
    Client: Sync,
    Block: BlockT,
    Backend: BackendT<Block>,
{
}

impl<Client, Backend, Block, Config> ServerDeps<Client, Backend, Block, Config>
where
    Block: BlockT,
    Backend: BackendT<Block>,
{
    pub fn new(client: Arc<Client>, backend: Arc<Backend>) -> Self {
        Self {
            client,
            backend,
            _block: PhantomData,
        }
    }
}

impl<Client, Backend, Block, Config> ServerDeps<Client, Backend, Block, Config>
where
    Client: ProvideRuntimeApi<Block>,
    Client: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError>,
    Client: Send + Sync + 'static,
    Config: pns_registrar::registrar::Config + pns_resolvers::resolvers::Config,
    Client::Api: PnsStorageApi<
        Block,
        Config::Moment,
        BalanceOf<Config>,
        Config::AccountId,
    >,
    Client::Api: BlockBuilder<Block>,
    Block: BlockT,
    Backend: BackendT<Block> + 'static,
{
    /// Start the HTTP REST API server on `socket`.
    ///
    /// Endpoints:
    /// - `GET /get_info/:id`   — look up a name record by namehash (H256)
    /// - `GET /info/:name`     — look up a name record by plain label (e.g. "alice")
    /// - `GET /all`            — return all registered name records
    pub async fn init_server(self, socket: impl Into<SocketAddr>) {
        let socket = socket.into();

        let app = Router::new()
            .route("/get_info/:id", get(Self::get_info))
            .route("/info/:name", get(Self::get_info_from_name))
            .route("/all", get(Self::all))
            .with_state(self);

        let listener = tokio::net::TcpListener::bind(socket).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    async fn get_info(
        State(state): State<Self>,
        Path(id): Path<DomainHash>,
    ) -> impl IntoResponse {
        let at = state.client.info().best_hash;
        let api = state.client.runtime_api();
        let res = match api.get_info(at, id) {
            Ok(res) => res,
            Err(e) => {
                error!("get_info error: {e:?}");
                None
            }
        };
        Json(res)
    }

    async fn get_info_from_name(
        State(state): State<Self>,
        Path(name): Path<String>,
    ) -> impl IntoResponse {
        let at = state.client.info().best_hash;
        let api = state.client.runtime_api();
        let res = Label::new_with_len(name.as_bytes())
            .map(|(label, _)| {
                use polkadot_sdk::frame_support::traits::Get;
                let basenode = <Config as pns_registrar::registrar::Config>::BaseNode::get();
                label.encode_with_node(&basenode)
            })
            .and_then(|id| match api.get_info(at, id) {
                Ok(res) => res,
                Err(e) => {
                    error!("get_info_from_name error: {e:?}");
                    None
                }
            });
        Json(res)
    }

    async fn all(State(state): State<Self>) -> impl IntoResponse {
        let at = state.client.info().best_hash;
        let api = state.client.runtime_api();
        let res = match api.all(at) {
            Ok(res) => res,
            Err(e) => {
                error!("all error: {e:?}");
                Vec::new()
            }
        };
        Json(res)
    }
}
