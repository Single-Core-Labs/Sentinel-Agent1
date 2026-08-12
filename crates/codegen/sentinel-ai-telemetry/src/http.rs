//! Origin/client identification used by the telemetry engine.
//!
//! [`OriginClientInfo`] is owned by `sentinel-ai-sampler` (so `SamplerConfig`
//! can use it without depending on shell). Re-exported here so the telemetry
//! engine can label events without depending on shell or sampler internals
//! beyond the type itself.

pub use sentinel_ai_sampler::OriginClientInfo;

/// Construct an [`OriginClientInfo`] from `AI_CLIENT_NAME` /
/// `AI_CLIENT_VERSION` env vars. Returns `None` when `AI_CLIENT_NAME`
/// is unset. Free function (not an inherent method) because the type lives
/// in another crate.
pub fn origin_client_info_from_env() -> Option<OriginClientInfo> {
    std::env::var("AI_CLIENT_NAME")
        .ok()
        .map(|product| OriginClientInfo {
            product,
            version: std::env::var("AI_CLIENT_VERSION").ok(),
        })
}
