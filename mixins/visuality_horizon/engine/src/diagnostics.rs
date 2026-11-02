//! Bounded numeric counters. Nothing here writes files or sends anything.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counters {
    /// Natural events observed: canonical hurt packets, landings, samples.
    pub hurt_canonical: u64,
    pub hurt_noncanonical: u64,
    pub hurt_throttled: u64,
    pub hurt_unmapped: u64,
    pub slime_landings: u64,
    pub block_samples: u64,
    pub water_samples: u64,
    pub water_rejected: u64,
    pub rejected_distance: u64,
    pub rejected_quality: u64,
    pub rejected_invalid: u64,
    pub dropped_queue: u64,
    pub dropped_budget: u64,
    pub replaced: u64,
    pub spawned: u64,
    pub family_spawned: [u64; 8],
    pub expired: u64,
    pub active_high_water: u32,
    pub queue_high_water: u32,
    pub sessions: u64,
    pub atlas_uploads: u64,
    pub pipelines_built: u64,
    pub draws: u64,
    pub vertices: u64,
    pub uploaded_bytes: u64,
    pub last_tick_us: u32,
    pub last_frame_us: u32,
    pub last_draw_us: u32,
}

impl Counters {
    /// One short line for the local diagnostics command.
    pub fn summary(&self, active: usize) -> String {
        format!(
            "active {active} (max {}), spawned {}, expired {}, dropped budget {} queue {}, \
             replaced {}, distance {}, quality {}, hurts {}/{} throttled {}, landings {}, \
             draws {}, tick {}us frame {}us draw {}us, by effect {:?}",
            self.active_high_water,
            self.spawned,
            self.expired,
            self.dropped_budget,
            self.dropped_queue,
            self.replaced,
            self.rejected_distance,
            self.rejected_quality,
            self.hurt_canonical,
            self.hurt_noncanonical,
            self.hurt_throttled,
            self.slime_landings,
            self.draws,
            self.last_tick_us,
            self.last_frame_us,
            self.last_draw_us,
            self.family_spawned
        )
    }
}
