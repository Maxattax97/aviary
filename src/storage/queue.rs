//! Durable ordering and requester-visible decision events.

use super::{PageBuilder, Storage};
use crate::{
    error::{AppError, Result},
    protocol::DecisionRequest,
};
use futures_util::TryStreamExt;
use serde_json::{json, Value};
use sqlx::{Sqlite, Transaction};

impl Storage {
    pub(super) async fn initialize_queue(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS decision_queue_clock (singleton INTEGER PRIMARY KEY CHECK(singleton = 1), position INTEGER NOT NULL)").execute(&mut *tx).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS decision_queue_positions (request_id INTEGER PRIMARY KEY, position INTEGER NOT NULL, version INTEGER NOT NULL DEFAULT 0, FOREIGN KEY(request_id) REFERENCES requests(id))").execute(&mut *tx).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS decision_events (id INTEGER PRIMARY KEY, request_id INTEGER NOT NULL, agent TEXT NOT NULL, kind TEXT NOT NULL, FOREIGN KEY(request_id) REFERENCES requests(id))").execute(&mut *tx).await?;
        let initialized = sqlx::query(
            "INSERT OR IGNORE INTO decision_queue_clock(singleton, position) VALUES (1, 0)",
        )
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if initialized {
            // Preserve old active-before-deferred placement for each priority.
            sqlx::query("INSERT INTO decision_queue_positions(request_id, position) SELECT r.id, row_number() OVER (ORDER BY COALESCE(a.deferred, 0), r.id) FROM requests r LEFT JOIN request_attention a ON a.request_id = r.id").execute(&mut *tx).await?;
            sqlx::query("UPDATE decision_queue_clock SET position = COALESCE((SELECT MAX(position) FROM decision_queue_positions), 0) WHERE singleton = 1").execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn enqueue_decision(tx: &mut Transaction<'_, Sqlite>, id: i64) -> Result<()> {
        let position = Self::next_position(tx).await?;
        sqlx::query("INSERT INTO decision_queue_positions(request_id, position) VALUES (?, ?)")
            .bind(id)
            .bind(position)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    async fn next_position(tx: &mut Transaction<'_, Sqlite>) -> Result<i64> {
        let position: i64 = sqlx::query_scalar("UPDATE decision_queue_clock SET position = position + 1 WHERE singleton = 1 RETURNING position").fetch_one(&mut **tx).await?;
        Ok(position)
    }

    pub(super) async fn decision_queue(&self, offset: usize) -> Result<Value> {
        let missing: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM requests r LEFT JOIN decision_queue_positions p ON p.request_id = r.id WHERE r.answer IS NULL AND p.request_id IS NULL)")
            .fetch_one(&self.pool).await?;
        if missing {
            return Err(AppError::QueueIndexIncomplete);
        }
        let mut page = PageBuilder::new(offset);
        let mut more = false;
        if let Ok(offset) = i64::try_from(offset) {
            let mut rows = sqlx::query_as::<_, (i64, String, i64, i64, i64)>("SELECT r.id, r.payload, COALESCE(a.importance, json_extract(r.payload, '$.importance'), 0), p.position, p.version FROM requests r JOIN decision_queue_positions p ON p.request_id = r.id LEFT JOIN request_attention a ON a.request_id = r.id WHERE r.answer IS NULL ORDER BY 3 DESC, p.position, r.id LIMIT -1 OFFSET ?").bind(offset).fetch(&self.pool);
            while let Some((id, payload, importance, position, version)) = rows.try_next().await? {
                let request: DecisionRequest = serde_json::from_str(&payload)?;
                if !page.push(json!({"id": id, "request": request, "importance": importance, "position": position, "version": version, "deferred": false}))? { more = true; break; }
            }
        }
        let newest: Option<i64> = sqlx::query_scalar("SELECT MAX(id) FROM requests")
            .fetch_one(&self.pool)
            .await?;
        let mut result = page.finish("requests", more);
        result
            .as_object_mut()
            .ok_or(AppError::InvalidFrame)?
            .insert("newest_id".into(), json!(newest));
        Ok(result)
    }

    pub(super) async fn defer_decision(&self, id: i64, identity: String) -> Result<Value> {
        if id <= 0 || identity.is_empty() {
            return Err(AppError::InvalidRequest);
        }
        let payload = serde_json::to_string(&json!({"id": id}))?;
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO submission_keys(identity, kind, payload) VALUES (?, 'defer_decision', ?) ON CONFLICT(identity) DO NOTHING").bind(&identity).bind(&payload).execute(&mut *tx).await?.rows_affected() == 1;
        let event = if inserted {
            let request: Option<(String,)> =
                sqlx::query_as("SELECT payload FROM requests WHERE id = ? AND answer IS NULL")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let request: DecisionRequest =
                serde_json::from_str(&request.ok_or(AppError::RequestNotFound)?.0)?;
            let position = Self::next_position(&mut tx).await?;
            let updated = sqlx::query("UPDATE decision_queue_positions SET position = ?, version = version + 1 WHERE request_id = ?").bind(position).bind(id).execute(&mut *tx).await?;
            if updated.rows_affected() != 1 {
                return Err(AppError::RequestNotFound);
            }
            let event = sqlx::query(
                "INSERT INTO decision_events(request_id, agent, kind) VALUES (?, ?, 'deferred')",
            )
            .bind(id)
            .bind(&request.agent)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            sqlx::query("UPDATE submission_keys SET record_id = ? WHERE identity = ?")
                .bind(event)
                .bind(&identity)
                .execute(&mut *tx)
                .await?;
            event
        } else {
            Self::retry_id(&mut tx, &identity, "defer_decision", &payload).await?
        };
        tx.commit().await?;
        Ok(json!({"id": id, "event_id": event}))
    }

    pub(super) async fn swap_decisions(
        &self,
        first: i64,
        first_version: i64,
        second: i64,
        second_version: i64,
        identity: String,
    ) -> Result<Value> {
        if first <= 0
            || second <= 0
            || first == second
            || first_version < 0
            || second_version < 0
            || identity.is_empty()
        {
            return Err(AppError::InvalidRequest);
        }
        let payload = serde_json::to_string(
            &json!({"first": first, "first_version": first_version, "second": second, "second_version": second_version}),
        )?;
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO submission_keys(identity, kind, payload) VALUES (?, 'swap_decisions', ?) ON CONFLICT(identity) DO NOTHING").bind(&identity).bind(&payload).execute(&mut *tx).await?.rows_affected() == 1;
        if inserted {
            let read = |id| {
                sqlx::query_as::<_, (i64, i64, i64)>("SELECT p.position, p.version, COALESCE(a.importance, json_extract(r.payload, '$.importance'), 0) FROM requests r JOIN decision_queue_positions p ON p.request_id = r.id LEFT JOIN request_attention a ON a.request_id = r.id WHERE r.id = ? AND r.answer IS NULL").bind(id)
            };
            let a = read(first)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(AppError::StaleQueue)?;
            let b = read(second)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(AppError::StaleQueue)?;
            if a.1 != first_version || b.1 != second_version || a.2 != b.2 {
                return Err(AppError::StaleQueue);
            }
            sqlx::query("UPDATE decision_queue_positions SET position = CASE request_id WHEN ? THEN ? ELSE ? END, version = version + 1 WHERE request_id IN (?, ?)").bind(first).bind(b.0).bind(a.0).bind(first).bind(second).execute(&mut *tx).await?;
            sqlx::query("UPDATE submission_keys SET record_id = ? WHERE identity = ?")
                .bind(first)
                .bind(&identity)
                .execute(&mut *tx)
                .await?;
        } else {
            Self::retry_id(&mut tx, &identity, "swap_decisions", &payload).await?;
        }
        tx.commit().await?;
        Ok(json!({"first": first, "second": second}))
    }

    pub(super) async fn decision_status(&self, id: i64, after: i64) -> Result<Value> {
        if id <= 0 || after < 0 {
            return Err(AppError::InvalidRequest);
        }
        let row: Option<(Option<String>, Option<i64>)> = sqlx::query_as("SELECT r.answer, (SELECT MIN(e.id) FROM decision_events e WHERE e.request_id = r.id AND e.id > ?) FROM requests r WHERE r.id = ?")
            .bind(after).bind(id).fetch_optional(&self.pool).await?;
        let (answer, event) = row.ok_or(AppError::RequestNotFound)?;
        let answer = answer
            .map(|value| serde_json::from_str::<Value>(&value))
            .transpose()?;
        Ok(json!({"id": id, "answer": answer, "deferred_event": event}))
    }

    pub(super) async fn decision_events(&self, agent: String, after: i64) -> Result<Value> {
        if agent.is_empty() || after < 0 {
            return Err(AppError::InvalidRequest);
        }
        let mut page = PageBuilder::new(0);
        let mut more = false;
        let mut last = after;
        let mut rows = sqlx::query_as::<_, (i64, i64, String)>("SELECT id, request_id, kind FROM decision_events WHERE agent = ? AND id > ? ORDER BY id").bind(agent).bind(after).fetch(&self.pool);
        while let Some((id, request_id, kind)) = rows.try_next().await? {
            if !page.push(json!({"id": id, "request_id": request_id, "kind": kind}))? {
                more = true;
                break;
            }
            last = id;
        }
        let mut result = page.finish("events", false);
        let object = result.as_object_mut().ok_or(AppError::InvalidFrame)?;
        object.remove("next_offset");
        object.insert(
            "next_after".into(),
            json!(if more { Some(last) } else { None }),
        );
        Ok(result)
    }
}
