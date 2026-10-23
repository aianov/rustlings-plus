pub const FEED_MAX_ENV: &str = "FEED_MAX";

pub const CELEBRITY_ENV: &str = "FEED_CELEBRITY";

pub const INLINE_ENV: &str = "FEED_FANOUT_INLINE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedLimits {
    pub max: usize,
    pub celebrity: usize,
    pub inline: usize,
}

impl Default for FeedLimits {
    fn default() -> FeedLimits {
        FeedLimits {
            max: 500,
            celebrity: 1_000,
            inline: 256,
        }
    }
}
