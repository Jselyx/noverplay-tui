use super::WaveMode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveQueueQuotas {
    pub core: usize,
    pub related: usize,
    pub favorites: usize,
    pub discovery: usize,
}

impl WaveQueueQuotas {
    pub fn calculate(queue_size: usize, mode: WaveMode, novelty: f64) -> Self {
        let safe_size = queue_size.max(1);
        let safe_novelty = novelty.clamp(0.0, 1.0);
        let ratios = match mode {
            WaveMode::Discovery => (0.20, 0.24, 0.04, 0.52),
            WaveMode::Favorites => (0.34, 0.24, 0.22, 0.20),
            WaveMode::Radio => (0.20, 0.58, 0.04, 0.18),
            WaveMode::Balanced => {
                let discovery = (0.20 + (safe_novelty - 0.35) * 0.2).clamp(0.10, 0.40);
                let related = 0.40;
                let favorites = 0.25;
                let core = (1.0_f64 - related - favorites - discovery).max(0.05);
                (core, related, favorites, discovery)
            }
        };
        let mut quotas = Self {
            core: quota_count(safe_size, ratios.0),
            related: quota_count(safe_size, ratios.1),
            favorites: quota_count(safe_size, ratios.2),
            discovery: quota_count(safe_size, ratios.3),
        };
        while quotas.total() > safe_size {
            if quotas.favorites > 0 {
                quotas.favorites -= 1;
            } else if quotas.core > 0 {
                quotas.core -= 1;
            } else if quotas.related > 0 {
                quotas.related -= 1;
            } else if quotas.discovery > 0 {
                quotas.discovery -= 1;
            }
        }
        while quotas.total() < safe_size {
            if safe_novelty >= 0.5 {
                quotas.discovery += 1;
            } else {
                quotas.related += 1;
            }
        }
        quotas
    }

    pub const fn total(self) -> usize {
        self.core + self.related + self.favorites + self.discovery
    }
}

fn quota_count(queue_size: usize, ratio: f64) -> usize {
    ((queue_size as f64) * ratio).round().max(0.0) as usize
}
