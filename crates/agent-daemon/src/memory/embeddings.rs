use agent_core::MemoryRecord;

use crate::AppState;

/// Compute and store an embedding for the given memory if the embedding provider is configured.
pub(super) async fn maybe_compute_embedding(
    state: &AppState,
    memory: &MemoryRecord,
) -> anyhow::Result<()> {
    let config = state.config.read().await;
    if !config.embedding.enabled {
        return Ok(());
    }
    let provider_id = config
        .embedding
        .provider_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("embedding provider_id not configured"))?;
    let provider = config
        .get_provider(provider_id)
        .ok_or_else(|| anyhow::anyhow!("embedding provider '{}' not found", provider_id))?
        .clone();
    let model = config
        .embedding
        .model
        .clone()
        .ok_or_else(|| anyhow::anyhow!("embedding model not configured"))?;
    let dimensions = config.embedding.dimensions;
    drop(config);

    let text = format!("{}: {}", memory.subject, memory.content);
    let dims = if dimensions > 0 {
        Some(dimensions)
    } else {
        None
    };
    let embedding =
        agent_providers::compute_embedding(&state.http_client, &provider, &model, &text, dims)
            .await?;
    state
        .storage
        .upsert_memory_embedding(&memory.id, &embedding, &model)?;
    Ok(())
}

/// Search for memories using embedding similarity.
pub(super) async fn embedding_search(
    state: &AppState,
    query: &str,
    workspace_key: Option<&str>,
    provider_id: Option<&str>,
    limit: usize,
    exclude_ids: &[String],
) -> anyhow::Result<Vec<MemoryRecord>> {
    let config = state.config.read().await;
    if !config.embedding.enabled {
        return Ok(Vec::new());
    }
    if !state.storage.has_memory_embeddings()? {
        return Ok(Vec::new());
    }

    let emb_provider_id = config
        .embedding
        .provider_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("embedding provider_id not configured"))?;
    let emb_provider = config
        .get_provider(emb_provider_id)
        .ok_or_else(|| anyhow::anyhow!("embedding provider '{}' not found", emb_provider_id))?
        .clone();
    let model = config
        .embedding
        .model
        .clone()
        .ok_or_else(|| anyhow::anyhow!("embedding model not configured"))?;
    let dimensions = config.embedding.dimensions;
    drop(config);

    let dims = if dimensions > 0 {
        Some(dimensions)
    } else {
        None
    };
    let query_embedding =
        agent_providers::compute_embedding(&state.http_client, &emb_provider, &model, query, dims)
            .await?;
    state.storage.search_memories_by_embedding(
        &query_embedding,
        workspace_key,
        provider_id,
        limit,
        exclude_ids,
    )
}
