use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tracing::debug;

/// Atomic counters covering all DNS drop reasons, cache events, and response types.
/// Logged at DEBUG level every 60 seconds. Prometheus integration can be added
/// by reading these counters from a metrics endpoint.
#[derive(Default)]
pub struct DnsMetrics {
    // ── Drop reasons ────────────────────────────────────────────────────────
    pub drop_inflight_cap:      AtomicU64,
    pub drop_any_query:         AtomicU64,
    pub drop_src_rate_limit:    AtomicU64,
    pub drop_zone_rate_limit:   AtomicU64,
    pub drop_penalty_backoff:   AtomicU64,
    pub drop_bloom_pre_filter:  AtomicU64,
    pub drop_storage_timeout:   AtomicU64,
    pub drop_storage_rate_limit:AtomicU64,
    pub drop_label_validation:  AtomicU64,
    pub drop_cname_depth:       AtomicU64,

    // ── Cache ────────────────────────────────────────────────────────────────
    pub neg_cache_hit:   AtomicU64,
    pub first_seen_miss: AtomicU64,   // first query for a name (admitted to first-seen cache)
    pub first_seen_hit:  AtomicU64,   // second query — promoted to negative cache

    // ── Storage ──────────────────────────────────────────────────────────────
    pub storage_lookups:  AtomicU64,
    pub storage_timeouts: AtomicU64,

    // ── Response types ───────────────────────────────────────────────────────
    pub resp_noerror:  AtomicU64,
    pub resp_nxdomain: AtomicU64,
    pub resp_refused:  AtomicU64,
    pub resp_servfail: AtomicU64,
    pub resp_notimp:   AtomicU64,
}

impl DnsMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn log_snapshot(&self) {
        debug!(
            drop_inflight_cap       = self.drop_inflight_cap.load(Ordering::Relaxed),
            drop_any_query          = self.drop_any_query.load(Ordering::Relaxed),
            drop_src_rate_limit     = self.drop_src_rate_limit.load(Ordering::Relaxed),
            drop_zone_rate_limit    = self.drop_zone_rate_limit.load(Ordering::Relaxed),
            drop_penalty_backoff    = self.drop_penalty_backoff.load(Ordering::Relaxed),
            drop_bloom_pre_filter   = self.drop_bloom_pre_filter.load(Ordering::Relaxed),
            drop_storage_timeout    = self.drop_storage_timeout.load(Ordering::Relaxed),
            drop_storage_rate_limit = self.drop_storage_rate_limit.load(Ordering::Relaxed),
            drop_label_validation   = self.drop_label_validation.load(Ordering::Relaxed),
            drop_cname_depth        = self.drop_cname_depth.load(Ordering::Relaxed),
            neg_cache_hit           = self.neg_cache_hit.load(Ordering::Relaxed),
            first_seen_miss         = self.first_seen_miss.load(Ordering::Relaxed),
            first_seen_hit          = self.first_seen_hit.load(Ordering::Relaxed),
            storage_lookups         = self.storage_lookups.load(Ordering::Relaxed),
            storage_timeouts        = self.storage_timeouts.load(Ordering::Relaxed),
            resp_noerror            = self.resp_noerror.load(Ordering::Relaxed),
            resp_nxdomain           = self.resp_nxdomain.load(Ordering::Relaxed),
            resp_refused            = self.resp_refused.load(Ordering::Relaxed),
            resp_servfail           = self.resp_servfail.load(Ordering::Relaxed),
            resp_notimp             = self.resp_notimp.load(Ordering::Relaxed),
            "pns_dns_metrics_snapshot"
        );
    }
}
