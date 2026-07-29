mod layout;
mod plan;

pub use layout::{ZapretInstall, ZapretKind, standard_linux_install};
pub use plan::ZapretPlan;

pub const SOUNDCLOUD_DOMAINS: [&str; 2] = ["soundcloud.com", "sndcdn.com"];

#[cfg(test)]
mod tests;
