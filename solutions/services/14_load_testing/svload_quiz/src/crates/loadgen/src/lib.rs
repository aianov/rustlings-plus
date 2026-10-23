pub mod hist;
pub mod http;
pub mod report;
pub mod run;
pub mod steps;

pub use hist::LogHistogram;
pub use http::HttpTarget;
pub use report::{Report, Tolerance, compare};
pub use run::{Target, run_closed, run_open};
pub use steps::{Limits, StepsOutcome, run_steps};

pub const HIST_BITS: u32 = 7;
