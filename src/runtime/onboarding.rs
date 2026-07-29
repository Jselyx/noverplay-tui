use std::path::PathBuf;

use tokio::task::JoinHandle;

use crate::onboarding::{
    probe_soundcloud,
    zapret::{ZapretInstall, ZapretPlan, apply_plan},
};

use super::message::RuntimeMessage;

pub(super) fn spawn_soundcloud_probe(
    sender: tokio::sync::mpsc::UnboundedSender<RuntimeMessage>,
    generation: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let access = probe_soundcloud().await;
        let _ = sender.send(RuntimeMessage::SoundCloudChecked { generation, access });
    })
}

pub(super) fn spawn_zapret_plan(
    sender: tokio::sync::mpsc::UnboundedSender<RuntimeMessage>,
    generation: u64,
    path: PathBuf,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            ZapretInstall::detect(path)
                .and_then(ZapretPlan::build)
                .map(Box::new)
                .map_err(|error| error.to_string())
        })
        .await
        .unwrap_or_else(|error| Err(format!("проверка Zapret упала: {error}")));
        let _ = sender.send(RuntimeMessage::ZapretPlanned { generation, result });
    })
}

pub(super) fn spawn_zapret_apply(
    sender: tokio::sync::mpsc::UnboundedSender<RuntimeMessage>,
    generation: u64,
    plan: ZapretPlan,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let result =
            tokio::task::spawn_blocking(move || apply_plan(&plan).map_err(|e| e.to_string()))
                .await
                .unwrap_or_else(|error| Err(format!("запись списка Zapret упала: {error}")));
        let _ = sender.send(RuntimeMessage::ZapretApplied { generation, result });
    })
}
