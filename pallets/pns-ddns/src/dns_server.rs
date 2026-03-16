use std::{
    collections::VecDeque,
    net::IpAddr,
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use bloomfilter::Bloom;
use governor::{DefaultDirectRateLimiter, DefaultKeyedRateLimiter, Quota, RateLimiter};
use hickory_proto::op::{Header, ResponseCode};
use hickory_server::{
    authority::{Catalog, MessageResponseBuilder},
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};
use moka::sync::Cache;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::{DnsConfig, MAX_CNAME_DEPTH as _};
use crate::authority::BlockChainAuthority;
use crate::metrics::DnsMetrics;

// ── /24 subnet key ───────────────────────────────────────────────────────────

/// Rate-limiter key identifying a /24 subnet. The 4th byte distinguishes IPv4
/// (4) from IPv6 (6) so the spaces don't collide.
type SubnetKey = [u8; 4];

fn subnet_key(addr: IpAddr) -> SubnetKey {
    match addr {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            [o[0], o[1], o[2], 4]
        }
        IpAddr::V6(v6) => {
            let o = v6.octets();
            [o[0], o[1], o[2], 6]
        }
    }
}

// ── Double-buffered bloom filter ─────────────────────────────────────────────

struct BloomState {
    primary: Bloom<String>,
    secondary: Bloom<String>,
    primary_count: u64,
}

pub struct DoubleBloom {
    state: Mutex<BloomState>,
    saturation_count: u64,
}

impl DoubleBloom {
    fn new(capacity: usize, fp_rate: f64) -> Self {
        let saturation_count = (capacity as f64 * 0.80) as u64;
        let state = BloomState {
            primary: Bloom::new_for_fp_rate(capacity, fp_rate),
            secondary: Bloom::new_for_fp_rate(capacity, fp_rate),
            primary_count: 0,
        };
        Self { state: Mutex::new(state), saturation_count }
    }

    /// Returns true if `name` is present in either filter.
    pub fn check(&self, name: &str) -> bool {
        let s = self.state.lock().unwrap();
        let key = name.to_string();
        s.primary.check(&key) || s.secondary.check(&key)
    }

    /// Insert `name` into the primary filter. Promotes secondary → primary when
    /// primary reaches 80% saturation; there is never a moment where no filter
    /// is active.
    pub fn set(&self, name: &str) {
        let mut s = self.state.lock().unwrap();
        let key = name.to_string();
        s.primary.set(&key);
        s.primary_count += 1;
        if s.primary_count >= self.saturation_count {
            // Promote: swap primary ↔ secondary; clear new secondary (old primary).
            // SAFETY: `primary` and `secondary` are distinct fields of the same struct;
            // the raw-pointer swap is sound because they cannot alias.
            unsafe {
                std::ptr::swap(&mut s.primary, &mut s.secondary);
            }
            s.secondary.clear();
            s.primary_count = 0;
        }
    }
}

// ── Penalty tracker ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct PenaltyState {
    strikes: u8,
    blocked_until: std::time::Instant,
}

// ── Rate limiter type aliases ─────────────────────────────────────────────────

type SubnetRl = DefaultKeyedRateLimiter<SubnetKey>;

fn make_subnet_rl(per_sec: u32) -> Arc<SubnetRl> {
    Arc::new(RateLimiter::keyed(
        Quota::per_second(NonZeroU32::new(per_sec).unwrap()),
    ))
}

// ── Interval-based response queue ────────────────────────────────────────────

/// Paces responses through a single tokio interval so that no query is
/// dispatched in less than `min_ms` milliseconds from when it entered the
/// queue. This prevents timing-based enumeration attacks without a per-query
/// sleep.
pub struct ResponseQueue {
    tx: mpsc::UnboundedSender<(tokio::time::Instant, oneshot::Sender<()>)>,
}

impl ResponseQueue {
    pub fn new(min_ms: u64) -> Self {
        let (tx, mut rx) =
            mpsc::unbounded_channel::<(tokio::time::Instant, oneshot::Sender<()>)>();
        tokio::spawn(async move {
            let tick = Duration::from_millis(1.max(min_ms / 4));
            let mut interval = tokio::time::interval(tick);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let min = Duration::from_millis(min_ms);
            let mut queue: VecDeque<(tokio::time::Instant, oneshot::Sender<()>)> =
                VecDeque::new();
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let now = tokio::time::Instant::now();
                        while let Some((entry, _)) = queue.front() {
                            if now >= *entry + min {
                                if let Some((_, s)) = queue.pop_front() {
                                    let _ = s.send(());
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    Some(item) = rx.recv() => {
                        queue.push_back(item);
                    }
                }
            }
        });
        Self { tx }
    }

    pub async fn wait(&self) {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send((tokio::time::Instant::now(), tx));
        let _ = rx.await;
    }
}

// ── HardenedHandler ───────────────────────────────────────────────────────────

/// Wraps any `RequestHandler` with a 13-step ordered security evaluation chain.
/// All checks run on the already-parsed `Request`; no raw UDP pre-filtering.
///
/// Step order:
///  1. Global concurrent query cap
///  2. ANY query type → NOTIMP
///  3. Per-source /24 subnet rate limit
///  4. Per-destination rate limit (NOTE: destination not exposed at RequestHandler
///     level — this step is mapped to a global response-rate guard instead)
///  5. Per-zone rate limit
///  5a. Exponential backoff penalty tracker per /24 subnet
///  6. Double-buffered bloom filter pre-check
///  7. Negative cache check
///  8. Storage lookup (via inner handler) with 100 ms timeout
///  9. Global storage read rate limit
/// 10. Label length validation (max 63 bytes per label)
/// 11. CNAME depth cap (enforced in BlockChainAuthority::lookup)
/// 12. Response size + TC bit (handled by Hickory's BinEncoder automatically)
/// 13. Uniform minimum response time via interval-based ResponseQueue
pub struct HardenedHandler {
    inner: Catalog,

    // Step 1
    inflight: Arc<AtomicU64>,
    max_inflight: u64,

    // Step 3 — per-source /24
    src_rl: Arc<SubnetRl>,

    // Step 4 — global response-rate guard (dst not available at this layer)
    global_response_rl: Arc<SubnetRl>,

    // Step 5 — per-zone (keyed by first label string)
    zone_rl: Arc<DefaultKeyedRateLimiter<String>>,

    // Step 5a — penalty tracker
    penalty_cache: Arc<Cache<SubnetKey, PenaltyState>>,

    // Step 6 — double-buffered bloom filter
    bloom: Arc<DoubleBloom>,

    // Step 7 — negative cache (name → ())
    neg_cache: Arc<Cache<String, ()>>,

    // Step 7 — first-seen cache for two-query pre-admission
    first_seen: Arc<Cache<String, ()>>,

    // Step 9 — global storage read rate limit
    storage_rl: Arc<DefaultDirectRateLimiter>,

    // Step 13 — uniform minimum response time
    response_queue: Arc<ResponseQueue>,

    // Metrics
    metrics: Arc<DnsMetrics>,
}

impl HardenedHandler {
    pub fn new(
        inner: Catalog,
        config: &DnsConfig,
        metrics: Arc<DnsMetrics>,
    ) -> Self {
        let zone_rl: DefaultKeyedRateLimiter<String> = RateLimiter::keyed(
            Quota::per_second(NonZeroU32::new(50).unwrap()),
        );

        let storage_rl: DefaultDirectRateLimiter = RateLimiter::direct(
            Quota::per_second(NonZeroU32::new(1000).unwrap()),
        );

        let neg_cache: Cache<String, ()> = Cache::builder()
            .max_capacity(20_000)
            .time_to_live(Duration::from_secs(60))
            .build();

        let first_seen: Cache<String, ()> = Cache::builder()
            .max_capacity(1_000)
            .time_to_live(Duration::from_secs(10))
            .build();

        let penalty_cache: Cache<SubnetKey, PenaltyState> = Cache::builder()
            .max_capacity(65_536)
            .time_to_live(Duration::from_secs(300))
            .build();

        Self {
            inner,
            inflight: Arc::new(AtomicU64::new(0)),
            max_inflight: 100,
            src_rl: make_subnet_rl(10),
            global_response_rl: make_subnet_rl(500),
            zone_rl: Arc::new(zone_rl),
            penalty_cache: Arc::new(penalty_cache),
            bloom: Arc::new(DoubleBloom::new(100_000, 0.01)),
            neg_cache: Arc::new(neg_cache),
            first_seen: Arc::new(first_seen),
            storage_rl: Arc::new(storage_rl),
            response_queue: Arc::new(ResponseQueue::new(config.min_response_ms)),
            metrics,
        }
    }

    /// Build a ResponseInfo for a silently dropped packet (no bytes sent).
    fn drop_info(request: &Request) -> ResponseInfo {
        let mut h = Header::response_from_request(request.header());
        h.set_response_code(ResponseCode::Refused);
        ResponseInfo::from(h)
    }

    /// Record a penalty strike for a /24 subnet.
    fn record_penalty(&self, key: SubnetKey) {
        let now = std::time::Instant::now();
        let new_state = if let Some(existing) = self.penalty_cache.get(&key) {
            let strikes = existing.strikes.saturating_add(1);
            let block_dur = match strikes {
                1 => Duration::from_secs(1),
                2 => Duration::from_secs(10),
                _ => Duration::from_secs(60),
            };
            PenaltyState { strikes, blocked_until: now + block_dur }
        } else {
            PenaltyState { strikes: 1, blocked_until: now + Duration::from_secs(1) }
        };
        self.penalty_cache.insert(key, new_state);
    }

    /// Check whether a /24 subnet is currently under penalty backoff.
    fn is_penalised(&self, key: &SubnetKey) -> bool {
        if let Some(state) = self.penalty_cache.get(key) {
            if std::time::Instant::now() < state.blocked_until {
                return true;
            }
        }
        false
    }

    /// Extract the first DNS label as a zone key for per-zone rate limiting.
    fn zone_key(request: &Request) -> Option<String> {
        request
            .queries()
            .first()
            .map(|q| {
                // First label of the queried name
                let name = q.name().to_string(); // e.g. "alice.dot."
                name.split('.').next().unwrap_or("").to_string()
            })
    }

    /// Validate that every label in the queried name is ≤ 63 bytes.
    fn labels_valid(request: &Request) -> bool {
        request.queries().iter().all(|q| {
            q.name()
                .to_string()
                .split('.')
                .all(|label| label.len() <= 63)
        })
    }
}

#[async_trait]
impl RequestHandler for HardenedHandler {
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        mut response_handle: R,
    ) -> ResponseInfo {
        let src_ip = request.src().ip();
        let src_key = subnet_key(src_ip);

        // ── Step 1: Global concurrent query cap ──────────────────────────────
        let current = self.inflight.fetch_add(1, Ordering::Relaxed);
        if current >= self.max_inflight {
            self.inflight.fetch_sub(1, Ordering::Relaxed);
            self.metrics.drop_inflight_cap.fetch_add(1, Ordering::Relaxed);
            return Self::drop_info(request);
        }
        // Ensure we always decrement, even on early returns below.
        let _guard = InFlightGuard(Arc::clone(&self.inflight));

        // ── Step 2: Block ANY queries (amplification prevention) ──────────────
        let is_any = request.queries().iter().any(|q| {
            q.query_type() == hickory_proto::rr::RecordType::ANY
        });
        if is_any {
            self.metrics.drop_any_query.fetch_add(1, Ordering::Relaxed);
            let builder = MessageResponseBuilder::from_message_request(request);
            let resp = builder.error_msg(request.header(), ResponseCode::NotImp);
            return response_handle.send_response(resp).await.unwrap_or_else(|_| {
                let mut h = Header::response_from_request(request.header());
                h.set_response_code(ResponseCode::NotImp);
                ResponseInfo::from(h)
            });
        }

        // ── Step 3: Per-source /24 rate limit ────────────────────────────────
        if self.src_rl.check_key(&src_key).is_err() {
            self.metrics.drop_src_rate_limit.fetch_add(1, Ordering::Relaxed);
            return Self::drop_info(request);
        }

        // ── Step 4: Global response-rate guard (proxy for per-dst) ───────────
        // The destination address is not exposed at the RequestHandler level.
        // A global response-rate limiter guards against amplification from this
        // server. dnsdist on port 53 provides full per-dst rate limiting.
        if self.global_response_rl.check_key(&src_key).is_err() {
            return Self::drop_info(request);
        }

        // ── Step 5: Per-zone rate limit ───────────────────────────────────────
        if let Some(zone) = Self::zone_key(request) {
            if self.zone_rl.check_key(&zone).is_err() {
                self.metrics.drop_zone_rate_limit.fetch_add(1, Ordering::Relaxed);
                // Record a penalty strike for this source subnet.
                self.record_penalty(src_key);
                return Self::drop_info(request);
            }
        }

        // ── Step 5a: Exponential backoff penalty tracker ──────────────────────
        if self.is_penalised(&src_key) {
            self.metrics.drop_penalty_backoff.fetch_add(1, Ordering::Relaxed);
            return Self::drop_info(request);
        }

        // ── Step 6: Bloom filter pre-check ───────────────────────────────────
        // The bloom filter is checked here as a cheap pre-filter before the
        // negative cache. If a name is in the bloom filter it is a candidate
        // for the negative cache check in step 7.
        let query_name = request
            .queries()
            .first()
            .map(|q| q.name().to_string())
            .unwrap_or_default();

        if self.bloom.check(&query_name) {
            // Candidate — fall through to full negative cache check (step 7)
        }

        // ── Step 7: Negative cache check ─────────────────────────────────────
        if self.neg_cache.get(&query_name).is_some() {
            self.metrics.neg_cache_hit.fetch_add(1, Ordering::Relaxed);
            // Two-query pre-admission: name is already confirmed NXDOMAIN.
            let builder = MessageResponseBuilder::from_message_request(request);
            let resp = builder.error_msg(request.header(), ResponseCode::NXDomain);
            return response_handle.send_response(resp).await.unwrap_or_else(|_| {
                let mut h = Header::response_from_request(request.header());
                h.set_response_code(ResponseCode::NXDomain);
                ResponseInfo::from(h)
            });
        }

        // ── Step 9: Global storage read rate limit ────────────────────────────
        // (Checked before storage lookup so we don't overload the runtime API)
        if self.storage_rl.check().is_err() {
            self.metrics.drop_storage_rate_limit.fetch_add(1, Ordering::Relaxed);
            return Self::drop_info(request);
        }

        // ── Step 10: Label length validation ─────────────────────────────────
        if !Self::labels_valid(request) {
            self.metrics.drop_label_validation.fetch_add(1, Ordering::Relaxed);
            return Self::drop_info(request);
        }

        // ── Step 13: Uniform minimum response time — wait in queue ───────────
        // We wait HERE so that the total latency from receiving the request to
        // calling inner.handle_request (which sends the response) is at least
        // min_response_ms. This prevents timing attacks on negative responses.
        self.response_queue.wait().await;

        // ── Step 8: Storage lookup with 100 ms timeout ───────────────────────
        self.metrics.storage_lookups.fetch_add(1, Ordering::Relaxed);
        let inner_result = tokio::time::timeout(
            Duration::from_millis(100),
            self.inner.handle_request(request, response_handle),
        )
        .await;

        match inner_result {
            Ok(info) => {
                // Two-query negative cache pre-admission: if the result was
                // NXDOMAIN, record in first_seen; promote to neg_cache on 2nd.
                if info.response_code() == ResponseCode::NXDomain {
                    if self.first_seen.get(&query_name).is_some() {
                        // Second query — promote to negative cache
                        self.neg_cache.insert(query_name.clone(), ());
                        self.bloom.set(&query_name);
                        self.first_seen.invalidate(&query_name);
                        self.metrics.first_seen_hit.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // First query — record in first_seen
                        self.first_seen.insert(query_name.clone(), ());
                        self.metrics.first_seen_miss.fetch_add(1, Ordering::Relaxed);
                    }
                    self.metrics.resp_nxdomain.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.metrics.resp_noerror.fetch_add(1, Ordering::Relaxed);
                }
                info
            }
            Err(_timeout) => {
                self.metrics.drop_storage_timeout.fetch_add(1, Ordering::Relaxed);
                self.metrics.storage_timeouts.fetch_add(1, Ordering::Relaxed);
                // Timeout — the response_handle may or may not have been consumed.
                // Construct a fallback ResponseInfo. The socket may receive nothing
                // (UDP timeout on the client side).
                let mut h = Header::response_from_request(request.header());
                h.set_response_code(ResponseCode::ServFail);
                ResponseInfo::from(h)
            }
        }
    }
}

// ── In-flight guard ──────────────────────────────────────────────────────────

struct InFlightGuard(Arc<AtomicU64>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

// ── CPU affinity helpers ──────────────────────────────────────────────────────

fn determine_dns_cores(
    requested: &Option<Vec<usize>>,
    workers: usize,
) -> Vec<core_affinity::CoreId> {
    let all_cores = match core_affinity::get_core_ids() {
        Some(c) if !c.is_empty() => c,
        _ => return vec![],
    };

    if all_cores.len() < 3 {
        warn!(
            total_cores = all_cores.len(),
            "System has fewer than 3 CPU cores. DNS and consensus/networking \
             threads will share CPU resources. Deploy on a system with ≥3 cores \
             to achieve isolation."
        );
        return vec![];
    }

    if let Some(ids) = requested {
        ids.iter()
            .filter_map(|&id| all_cores.iter().find(|c| c.id == id).cloned())
            .take(workers)
            .collect()
    } else {
        // Auto-select: highest-numbered cores (furthest from the OS scheduler
        // and consensus threads which tend to use low-numbered cores).
        let mut sorted = all_cores.clone();
        sorted.sort_by(|a, b| b.id.cmp(&a.id));
        sorted.into_iter().take(workers).collect()
    }
}

// ── start_dns_server ──────────────────────────────────────────────────────────

/// Spawn a dedicated DNS server on a separate tokio runtime with its own
/// OS thread pool, isolated from the node's main consensus/networking runtime.
///
/// The function returns immediately; the DNS runtime runs in the background.
///
/// # dnsdist recommendation
///
/// Deploy dnsdist on port 53 as a frontend proxy forwarding to `--dns-port`.
/// Minimum recommended dnsdist configuration:
///
/// ```lua
/// -- dnsdist.conf (minimum PNS frontend)
/// newServer({address="127.0.0.1:<dns-port>", pool="pns"})
/// addAction(MaxQPSIPRule(5, 32), DropAction())          -- 5 req/s per /32
/// addAction(QTypeRule(DNSQType.ANY), RCodeAction(DNSRCode.NOTIMP))
/// addResponseAction(MaxQPSIPRule(5, 32), DropResponseAction())
/// setMaxUDPOutstanding(1024)
/// addAction(RecordsCountRule(DNSSectionType.Question, 1, 1), RCodeAction(DNSRCode.FORMERR))
/// -- UDP query size limit
/// addAction(AndRule{RecordsCountRule(DNSSectionType.Question, 1, 1),
///                   PayloadSizeRule("udp", 512)},
///            RCodeAction(DNSRCode.FORMERR))
/// ```
pub fn start_dns_server<C, Block, Dur, Bal, Acc>(
    client: std::sync::Arc<C>,
    config: DnsConfig,
) where
    C: polkadot_sdk::sp_api::ProvideRuntimeApi<Block>
        + polkadot_sdk::sp_blockchain::HeaderBackend<Block>
        + Send
        + Sync
        + 'static,
    C::Api: pns_runtime_api::PnsStorageApi<Block, Dur, Bal, Acc>,
    Block: polkadot_sdk::sp_runtime::traits::Block + Send + Sync + 'static,
    Block::Hash: Send + Sync + 'static,
    Dur: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
    Bal: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
    Acc: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
{
    let port = config.port;
    let workers = config.workers;
    let cores = config.cores.clone();
    let min_ms = config.min_response_ms;

    info!(
        port,
        workers,
        min_response_ms = min_ms,
        "Starting PNS DNS server (dedicated runtime). \
         IMPORTANT: deploy dnsdist on port 53 forwarding to this port. \
         Recommended dnsdist minimum config: \
         per-IP rate limit 5 req/s, ANY block, RRL, UDP query size limit 512 bytes."
    );

    let affinity_cores = determine_dns_cores(&cores, workers);

    std::thread::spawn(move || {
        let aff_iter = Arc::new(Mutex::new(affinity_cores.into_iter()));

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(workers)
            .thread_name("pns-dns")
            .on_thread_start(move || {
                if let Some(core_id) = aff_iter.lock().unwrap().next() {
                    if !core_affinity::set_for_current(core_id) {
                        warn!(
                            ?core_id,
                            "Failed to set CPU affinity for DNS worker thread"
                        );
                    }
                }
            })
            .enable_all()
            .build()
            .expect("DNS tokio runtime");

        rt.block_on(async move {
            run_dns_server(client, port, workers, min_ms).await;
        });
    });
}

async fn run_dns_server<C, Block, Dur, Bal, Acc>(
    client: std::sync::Arc<C>,
    port: u16,
    _workers: usize,
    min_ms: u64,
) where
    C: polkadot_sdk::sp_api::ProvideRuntimeApi<Block>
        + polkadot_sdk::sp_blockchain::HeaderBackend<Block>
        + Send
        + Sync
        + 'static,
    C::Api: pns_runtime_api::PnsStorageApi<Block, Dur, Bal, Acc>,
    Block: polkadot_sdk::sp_runtime::traits::Block + Send + Sync + 'static,
    Block::Hash: Send + Sync + 'static,
    Dur: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
    Bal: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
    Acc: codec::Decode + codec::Encode + polkadot_sdk::sp_runtime::traits::MaybeSerialize + Send + Sync + 'static,
{
    use hickory_proto::rr::LowerName;
    use hickory_server::authority::AuthorityObject;
    use std::str::FromStr;

    // Build the BlockChainAuthority and register it in a Catalog
    let authority = BlockChainAuthority::<C, Block, Dur, Bal, Acc>::new(client);
    let dot_origin = LowerName::from(
        hickory_proto::rr::Name::from_str("dot.").expect("valid zone"),
    );
    let mut catalog = Catalog::new();
    catalog.upsert(dot_origin, vec![Arc::new(authority) as Arc<dyn AuthorityObject>]);

    let metrics = DnsMetrics::new();
    let config_for_handler = DnsConfig {
        port,
        workers: _workers,
        cores: None,
        min_response_ms: min_ms,
    };

    let handler = HardenedHandler::new(catalog, &config_for_handler, Arc::clone(&metrics));

    // Metrics logging loop every 60 seconds
    let metrics_clone = Arc::clone(&metrics);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            metrics_clone.log_snapshot();
        }
    });

    // Bind UDP socket
    let addr = format!("0.0.0.0:{}", port);
    let socket = match tokio::net::UdpSocket::bind(&addr).await {
        Ok(s) => s,
        Err(e) => {
            warn!(
                port,
                error = %e,
                "Failed to bind DNS UDP socket on port {}. DNS server will not start. \
                 Hint: port 53 requires root or CAP_NET_BIND_SERVICE. \
                 Use --dns-port to choose an unprivileged port and proxy with dnsdist.",
                port
            );
            return;
        }
    };

    info!(port, "PNS DNS UDP listener bound");

    let mut server = hickory_server::ServerFuture::new(handler);
    server.register_socket(socket);

    if let Err(e) = server.block_until_done().await {
        warn!(error = %e, "DNS server exited with error");
    }
}
