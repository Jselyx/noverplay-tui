use std::{fs, io::ErrorKind};

use anyhow::{Context, Result};

use super::{SOUNDCLOUD_DOMAINS, ZapretInstall};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZapretPlan {
    pub install: ZapretInstall,
    pub additions: Vec<String>,
    original: String,
    updated: String,
}

impl ZapretPlan {
    pub fn build(install: ZapretInstall) -> Result<Self> {
        let original = match fs::read_to_string(&install.list_path) {
            Ok(value) => value,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Не удалось прочитать {}", install.list_path.display())
                });
            }
        };
        let known = original
            .lines()
            .filter_map(normalize_domain_line)
            .collect::<Vec<_>>();
        let additions = SOUNDCLOUD_DOMAINS
            .iter()
            .filter(|domain| !known.iter().any(|known| known == **domain))
            .map(|domain| (*domain).to_string())
            .collect::<Vec<_>>();
        let updated = append_domains(&original, &additions);
        Ok(Self {
            install,
            additions,
            original,
            updated,
        })
    }

    pub fn has_changes(&self) -> bool {
        !self.additions.is_empty()
    }

    pub fn render_diff(&self) -> String {
        let path = self.install.list_path.display();
        let mut lines = vec![format!("--- {path}"), format!("+++ {path}")];
        if self.additions.is_empty() {
            lines.push("  изменений нет".to_string());
        } else {
            lines.extend(self.additions.iter().map(|domain| format!("+ {domain}")));
        }
        lines.join("\n")
    }

    pub fn original_contents(&self) -> &str {
        &self.original
    }

    pub fn resulting_contents(&self) -> &str {
        &self.updated
    }
}

fn normalize_domain_line(line: &str) -> Option<String> {
    let line = line.split_once('#').map_or(line, |(value, _)| value).trim();
    if line.is_empty() {
        None
    } else {
        Some(line.trim_start_matches('.').to_ascii_lowercase())
    }
}

fn append_domains(original: &str, additions: &[String]) -> String {
    if additions.is_empty() {
        return original.to_string();
    }
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut updated = original.to_string();
    if !updated.is_empty() && !updated.ends_with(['\n', '\r']) {
        // перенос строки тут не эстетика, иначе получим example.comsoundcloud.com и новый вид искусства
        updated.push_str(newline);
    }
    for domain in additions {
        updated.push_str(domain);
        updated.push_str(newline);
    }
    updated
}
