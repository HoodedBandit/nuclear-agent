#[derive(Debug, Default)]
pub(super) struct MemoryConflictResolution {
    pub(super) supersede_ids: Vec<String>,
}

#[derive(Debug, Default)]
pub(super) struct MemoryRebuildStats {
    pub(super) observations_scanned: usize,
    pub(super) memories_upserted: usize,
    pub(super) embeddings_refreshed: usize,
}
