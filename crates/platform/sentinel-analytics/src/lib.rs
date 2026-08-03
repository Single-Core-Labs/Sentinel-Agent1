pub mod accepted_lines;
pub mod capture;
pub mod client;
pub mod consent;
pub mod crash;
pub mod event;
pub mod events;
pub mod fact;
pub mod pipeline;
pub mod queue;
pub mod reducer;

pub use capture::*;
pub use client::*;
pub use consent::*;
pub use crash::*;
pub use event::*;
pub use events::*;
pub use fact::*;
pub use pipeline::*;
pub use queue::*;
pub use reducer::*;

// accepted_lines re-exports LineStats manually (ambiguous with events::LineStats)
pub use accepted_lines::fingerprint_diff;
pub use accepted_lines::fingerprint_lines;
pub use accepted_lines::line_stats;
pub use accepted_lines::parse_unified_diff;
pub use accepted_lines::DiffHunk;
