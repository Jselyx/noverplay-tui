use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};

use super::ZapretPlan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZapretApplyResult {
    pub list_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub added: Vec<String>,
}

pub fn apply_plan(plan: &ZapretPlan) -> Result<ZapretApplyResult> {
    if !plan.has_changes() {
        return Ok(ZapretApplyResult {
            list_path: plan.install.list_path.clone(),
            backup_path: None,
            added: Vec::new(),
        });
    }
    let current = read_current(&plan.install.list_path)?;
    if current != plan.original_contents() {
        bail!("список Zapret изменился после показа diff, собери план заново")
    }
    let backup_path = if plan.install.list_path.exists() {
        Some(create_backup(&plan.install.list_path)?)
    } else {
        None
    };
    write_list(&plan.install.list_path, plan.resulting_contents())?;
    Ok(ZapretApplyResult {
        list_path: plan.install.list_path.clone(),
        backup_path,
        added: plan.additions.clone(),
    })
}

pub fn sudo_command(path: &Path) -> String {
    let escaped = path.display().to_string().replace('\'', "'\"'\"'");
    format!("sudo noverplay setup-zapret --path '{escaped}'")
}

fn read_current(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(value),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(with_permission_hint(error, path, "прочитать")),
    }
}

fn create_backup(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .context("у списка Zapret нет родительской папки")?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("имя списка Zapret не читается")?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let bytes = fs::read(path).map_err(|error| with_permission_hint(error, path, "прочитать"))?;
    for suffix in 0..100_u8 {
        let backup = parent.join(format!("{name}.noverplay-{stamp}-{suffix}.bak"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
        {
            Ok(mut target) => {
                target
                    .write_all(&bytes)
                    .map_err(|error| with_permission_hint(error, path, "создать backup"))?;
                target
                    .sync_all()
                    .map_err(|error| with_permission_hint(error, path, "сохранить backup"))?;
                return Ok(backup);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(with_permission_hint(error, path, "создать backup")),
        }
    }
    bail!("не удалось подобрать свободное имя для backup списка Zapret")
}

fn write_list(path: &Path, contents: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|error| with_permission_hint(error, path, "открыть для записи"))?;
    file.write_all(contents.as_bytes())
        .map_err(|error| with_permission_hint(error, path, "записать"))?;
    file.sync_all()
        .map_err(|error| with_permission_hint(error, path, "сохранить"))
}

fn with_permission_hint(error: std::io::Error, path: &Path, action: &str) -> anyhow::Error {
    if error.kind() == ErrorKind::PermissionDenied {
        anyhow::anyhow!(
            "Нет прав, чтобы {action} {}\n{}",
            path.display(),
            sudo_command(path)
        )
    } else {
        anyhow::anyhow!("Не удалось {action} {}: {error}", path.display())
    }
}
