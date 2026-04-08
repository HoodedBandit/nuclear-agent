use super::*;

impl Storage {
    pub fn upsert_memory(&self, memory: &MemoryRecord) -> Result<()> {
        let connection = self.connection()?;
        let tags_json = serde_json::to_string(&memory.tags)?;
        let evidence_refs_json = serde_json::to_string(&memory.evidence_refs)?;
        let tags_text = memory.tags.join(" ");
        connection.execute(
            "
            INSERT INTO memory_records (
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, tags_text, identity_key, observation_source, superseded_by,
                review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
            ON CONFLICT(id) DO UPDATE SET
                kind_json = excluded.kind_json,
                scope_json = excluded.scope_json,
                subject = excluded.subject,
                content = excluded.content,
                confidence = excluded.confidence,
                updated_at = excluded.updated_at,
                last_used_at = excluded.last_used_at,
                source_session_id = COALESCE(excluded.source_session_id, memory_records.source_session_id),
                source_message_id = COALESCE(excluded.source_message_id, memory_records.source_message_id),
                provider_id = COALESCE(excluded.provider_id, memory_records.provider_id),
                workspace_key = COALESCE(excluded.workspace_key, memory_records.workspace_key),
                tags_json = excluded.tags_json,
                tags_text = excluded.tags_text,
                identity_key = COALESCE(excluded.identity_key, memory_records.identity_key),
                observation_source = COALESCE(excluded.observation_source, memory_records.observation_source),
                superseded_by = excluded.superseded_by,
                review_status_json = excluded.review_status_json,
                review_note = excluded.review_note,
                reviewed_at = excluded.reviewed_at,
                supersedes = excluded.supersedes,
                evidence_refs_json = excluded.evidence_refs_json
            ",
            params![
                memory.id,
                serde_json::to_string(&memory.kind)?,
                serde_json::to_string(&memory.scope)?,
                memory.subject,
                memory.content,
                i64::from(memory.confidence),
                memory.created_at.to_rfc3339(),
                memory.updated_at.to_rfc3339(),
                memory.last_used_at.map(|value| value.to_rfc3339()),
                memory.source_session_id,
                memory.source_message_id,
                memory.provider_id,
                memory.workspace_key,
                tags_json,
                tags_text,
                memory.identity_key,
                memory.observation_source,
                memory.superseded_by,
                serde_json::to_string(&memory.review_status)?,
                memory.review_note,
                memory.reviewed_at.map(|value| value.to_rfc3339()),
                memory.supersedes,
                evidence_refs_json,
            ],
        )?;
        Ok(())
    }

    /// Store an embedding vector for a memory record.
    pub fn upsert_memory_embedding(
        &self,
        memory_id: &str,
        embedding: &[f32],
        model: &str,
    ) -> Result<()> {
        let connection = self.connection()?;
        let blob = embedding_to_blob(embedding);
        connection.execute(
            "
            INSERT INTO memory_embeddings (memory_id, embedding, model, dimensions, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(memory_id) DO UPDATE SET
                embedding = excluded.embedding,
                model = excluded.model,
                dimensions = excluded.dimensions,
                created_at = excluded.created_at
            ",
            params![
                memory_id,
                blob,
                model,
                embedding.len() as i64,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Find memories whose embeddings are most similar to the query embedding.
    /// Returns `(memory_id, similarity_score)` pairs sorted by descending similarity.
    pub fn search_memories_by_embedding(
        &self,
        query_embedding: &[f32],
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
        limit: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;

        let mut statement = connection.prepare(
            "
            SELECT me.memory_id, me.embedding,
                mr.id, mr.kind_json, mr.scope_json, mr.subject, mr.content, mr.confidence,
                mr.created_at, mr.updated_at, mr.last_used_at, mr.source_session_id,
                mr.source_message_id, mr.provider_id, mr.workspace_key, mr.tags_json,
                mr.identity_key, mr.observation_source, mr.superseded_by,
                mr.review_status_json, mr.review_note, mr.reviewed_at, mr.supersedes,
                mr.evidence_refs_json
            FROM memory_embeddings me
            JOIN memory_records mr ON mr.id = me.memory_id
            WHERE mr.superseded_by IS NULL
              AND mr.review_status_json = ?1
            ",
        )?;
        let accepted_json = serde_json::to_string(&MemoryReviewStatus::Accepted)?;
        let rows = statement.query_map(params![accepted_json], |row| {
            let embedding_blob: Vec<u8> = row.get(1)?;
            let memory = row_to_memory_at_offset(row, 2)?;
            Ok((embedding_blob, memory))
        })?;

        let query_norm = vector_norm(query_embedding);
        let mut scored: Vec<(f32, MemoryRecord)> = Vec::new();
        for row in rows {
            let (blob, memory) = row?;
            if exclude_ids.contains(&memory.id) {
                continue;
            }
            if !memory_matches_scope(&memory, workspace_key, provider_id) {
                continue;
            }
            let stored = blob_to_embedding(&blob);
            let similarity = cosine_similarity(query_embedding, &stored, query_norm);
            scored.push((similarity, memory));
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored
            .into_iter()
            .take(limit)
            .filter(|(score, _)| *score > 0.3)
            .map(|(_, memory)| memory)
            .collect())
    }

    pub fn has_memory_embeddings(&self) -> Result<bool> {
        let connection = self.connection()?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM memory_embeddings", [], |row| {
                row.get(0)
            })?;
        Ok(count > 0)
    }

    pub fn delete_memory_embedding(&self, memory_id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM memory_embeddings WHERE memory_id = ?1",
            params![memory_id],
        )?;
        Ok(())
    }

    pub fn get_memory(&self, memory_id: &str) -> Result<Option<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE id = ?1
            ",
        )?;
        statement
            .query_row([memory_id], row_to_memory)
            .optional()
            .map_err(Into::into)
    }

    pub fn list_memories(&self, limit: usize) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE superseded_by IS NULL
              AND review_status_json = ?2
            ORDER BY updated_at DESC
            LIMIT ?1
            ",
        )?;
        let rows = statement.query_map(
            params![
                limit as i64,
                serde_json::to_string(&MemoryReviewStatus::Accepted)?
            ],
            row_to_memory,
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_memories_by_review_status(
        &self,
        status: MemoryReviewStatus,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE superseded_by IS NULL
              AND review_status_json = ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            ",
        )?;
        let rows = statement.query_map(
            params![serde_json::to_string(&status)?, limit as i64],
            row_to_memory,
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn count_memories(&self) -> Result<usize> {
        let connection = self.connection()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM memory_records WHERE superseded_by IS NULL AND review_status_json = ?1",
            [serde_json::to_string(&MemoryReviewStatus::Accepted)?],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn count_memories_by_review_status(&self, status: MemoryReviewStatus) -> Result<usize> {
        let connection = self.connection()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM memory_records WHERE superseded_by IS NULL AND review_status_json = ?1",
            [serde_json::to_string(&status)?],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn list_memories_by_tag(
        &self,
        tag: &str,
        limit: usize,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE superseded_by IS NULL
              AND review_status_json = ?3
              AND tags_text LIKE ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            ",
        )?;
        let rows = statement.query_map(
            params![
                format!("%{tag}%"),
                limit as i64,
                serde_json::to_string(&MemoryReviewStatus::Accepted)?
            ],
            row_to_memory,
        )?;
        let memories = rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|memory| memory_matches_scope(memory, workspace_key, provider_id))
            .collect::<Vec<_>>();
        Ok(memories)
    }

    pub fn list_memories_by_source_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE superseded_by IS NULL
              AND review_status_json = ?1
              AND source_session_id = ?2
            ORDER BY updated_at DESC
            LIMIT ?3
            ",
        )?;
        let rows = statement.query_map(
            params![
                serde_json::to_string(&MemoryReviewStatus::Accepted)?,
                session_id,
                limit as i64
            ],
            row_to_memory,
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn forget_memory(&self, memory_id: &str) -> Result<bool> {
        let connection = self.connection()?;
        let deleted =
            connection.execute("DELETE FROM memory_records WHERE id = ?1", [memory_id])?;
        Ok(deleted > 0)
    }

    pub fn touch_memory(&self, memory_id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE memory_records SET last_used_at = ?2 WHERE id = ?1",
            params![memory_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn update_memory_review_status(
        &self,
        memory_id: &str,
        status: MemoryReviewStatus,
        note: Option<&str>,
    ) -> Result<bool> {
        let connection = self.connection()?;
        let reviewed_at = match status {
            MemoryReviewStatus::Candidate => Option::<String>::None,
            _ => Some(Utc::now().to_rfc3339()),
        };
        let updated = connection.execute(
            "UPDATE memory_records SET review_status_json = ?2, review_note = ?3, reviewed_at = ?4, updated_at = ?5 WHERE id = ?1",
            params![
                memory_id,
                serde_json::to_string(&status)?,
                note,
                reviewed_at,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn mark_memory_superseded(&self, memory_id: &str, superseded_by: &str) -> Result<bool> {
        let connection = self.connection()?;
        let updated = connection.execute(
            "UPDATE memory_records SET superseded_by = ?2, updated_at = ?3 WHERE id = ?1",
            params![memory_id, superseded_by, Utc::now().to_rfc3339()],
        )?;
        Ok(updated > 0)
    }

    pub fn find_memory_by_subject(
        &self,
        subject: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Option<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE lower(subject) = lower(?1)
              AND superseded_by IS NULL
            ORDER BY updated_at DESC
            ",
        )?;
        let rows = statement.query_map([subject], row_to_memory)?;
        for row in rows {
            let memory = row?;
            if memory_matches_scope(&memory, workspace_key, provider_id) {
                return Ok(Some(memory));
            }
        }
        Ok(None)
    }

    pub fn find_memory_by_identity_key(
        &self,
        identity_key: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Option<MemoryRecord>> {
        Ok(self
            .list_active_memories_by_identity_key(identity_key, workspace_key, provider_id)?
            .into_iter()
            .next())
    }

    pub fn list_active_memories_by_identity_key(
        &self,
        identity_key: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE lower(identity_key) = lower(?1)
              AND superseded_by IS NULL
            ORDER BY updated_at DESC
            ",
        )?;
        let rows = statement.query_map([identity_key], row_to_memory)?;
        let mut memories = rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|memory| memory_matches_scope(memory, workspace_key, provider_id))
            .collect::<Vec<_>>();
        memories.sort_by(|left, right| {
            memory_review_status_rank(left.review_status.clone())
                .cmp(&memory_review_status_rank(right.review_status.clone()))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        Ok(memories)
    }

    pub fn search_memories(
        &self,
        query: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
        review_statuses: &[MemoryReviewStatus],
        include_superseded: bool,
        limit: usize,
    ) -> Result<(Vec<MemoryRecord>, Vec<SessionSearchHit>)> {
        let connection = self.connection()?;
        let fts_query = normalize_fts_query(query);

        let expanded_query = build_expanded_fts_query(query);
        let effective_fts_query = if expanded_query.is_empty() {
            &fts_query
        } else {
            &expanded_query
        };

        let mut memories = if effective_fts_query.is_empty() {
            Vec::new()
        } else {
            let mut statement = connection.prepare(
                "
                SELECT
                    mr.id, mr.kind_json, mr.scope_json, mr.subject, mr.content, mr.confidence,
                    mr.created_at, mr.updated_at, mr.last_used_at, mr.source_session_id,
                    mr.source_message_id, mr.provider_id, mr.workspace_key, mr.tags_json,
                    mr.identity_key, mr.observation_source, mr.superseded_by, mr.review_status_json, mr.review_note, mr.reviewed_at, mr.supersedes, mr.evidence_refs_json
                FROM memory_records_fts
                JOIN memory_records mr ON mr.id = memory_records_fts.memory_id
                WHERE memory_records_fts MATCH ?1
                  AND (?3 = 1 OR mr.superseded_by IS NULL)
                ORDER BY bm25(memory_records_fts), mr.updated_at DESC
                LIMIT ?2
                ",
            )?;
            let rows = statement.query_map(
                params![
                    effective_fts_query,
                    limit as i64,
                    if include_superseded { 1 } else { 0 }
                ],
                row_to_memory,
            )?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|memory| memory_matches_scope(memory, workspace_key, provider_id))
                .filter(|memory| {
                    memory_matches_query_filters(memory, review_statuses, include_superseded)
                })
                .take(limit)
                .collect()
        };

        if memories.len() < limit / 2 {
            let exclude_ids: Vec<String> = memories.iter().map(|m| m.id.clone()).collect();
            let remaining = limit - memories.len();
            let fuzzy_results = fuzzy_memory_search(
                &connection,
                query,
                workspace_key,
                provider_id,
                review_statuses,
                include_superseded,
                remaining,
                &exclude_ids,
            )?;
            memories.extend(fuzzy_results);
        }

        let transcript_hits = if fts_query.is_empty() {
            Vec::new()
        } else {
            let mut statement = connection.prepare(
                "
                SELECT
                    m.session_id, m.id, m.role_json, m.content, m.created_at, m.provider_id,
                    m.model
                FROM messages_fts
                JOIN messages m ON m.id = messages_fts.message_id
                WHERE messages_fts MATCH ?1
                ORDER BY bm25(messages_fts), m.created_at DESC
                LIMIT ?2
                ",
            )?;
            let rows = statement.query_map(params![fts_query, limit as i64], |row| {
                let role_json: String = row.get(2)?;
                let created_at: String = row.get(4)?;
                let content: String = row.get(3)?;
                Ok(SessionSearchHit {
                    session_id: row.get(0)?,
                    message_id: row.get(1)?,
                    role: serde_json::from_str(&role_json).map_err(json_decode_error)?,
                    preview: summarize_preview(&content, 280),
                    created_at: parse_datetime(&created_at)?,
                    provider_id: row.get(5)?,
                    model: row.get(6)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        Ok((memories, transcript_hits))
    }
}
