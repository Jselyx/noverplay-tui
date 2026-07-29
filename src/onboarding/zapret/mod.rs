mod layout;

pub use layout::{ZapretInstall, ZapretKind, standard_linux_install};

pub const SOUNDCLOUD_DOMAINS: [&str; 2] = ["soundcloud.com", "sndcdn.com"];

#[cfg(test)]
mod tests;
