pub mod action;
pub mod app;
pub mod audio;
pub mod config;
pub mod effect;
pub mod event;
pub mod model;
pub mod provider;
pub mod secrets;
pub mod storage;
pub mod terminal;
pub mod ui;

pub const APP_NAME: &str = "Noverplay";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
