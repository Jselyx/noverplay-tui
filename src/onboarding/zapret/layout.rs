use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZapretKind {
    FlowsealWindows,
    SnowyLinux,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZapretInstall {
    pub kind: ZapretKind,
    pub root: PathBuf,
    pub list_path: PathBuf,
}

impl ZapretInstall {
    pub fn detect(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let root = root_from_hint(path);
        if root.join("bin").join("winws.exe").is_file() && root.join("lists").is_dir() {
            return Ok(Self {
                kind: ZapretKind::FlowsealWindows,
                list_path: root.join("lists").join("list-general-user.txt"),
                root,
            });
        }
        if root.join("ipset").is_dir() && linux_markers_exist(&root) {
            return Ok(Self {
                kind: ZapretKind::SnowyLinux,
                list_path: root.join("ipset").join("zapret-hosts-user.txt"),
                root,
            });
        }
        bail!(
            "{} не похож на установку Flowseal или Snowy-Fluffy",
            path.display()
        )
    }
}

fn root_from_hint(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case("list-general-user.txt")
            || name.eq_ignore_ascii_case("zapret-hosts-user.txt")
    }) {
        // файл вбили целиком, родительские папки теперь внезапно не загадка века
        return path
            .parent()
            .and_then(Path::parent)
            .unwrap_or(path)
            .to_path_buf();
    }
    path.to_path_buf()
}

fn linux_markers_exist(root: &Path) -> bool {
    ["nfq", "init.d"]
        .iter()
        .any(|name| root.join(name).is_dir())
        || root.join("install_easy.sh").is_file()
}

pub fn standard_linux_install() -> Result<ZapretInstall> {
    ZapretInstall::detect("/opt/zapret").context("Snowy-Fluffy не найден в /opt/zapret")
}
