use std::time::Duration;

use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};

use crate::{
    error::{AppError, Result},
    protocol::{AnswerType, DecisionRequest, Operation, PendingRequest, SuggestedAnswer},
};

#[derive(Clone)]
pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    pub async fn open(path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS requests (id INTEGER PRIMARY KEY, identity TEXT NOT NULL UNIQUE, payload TEXT NOT NULL, answer TEXT)")
            .execute(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn handle(&self, operation: Operation) -> Result<Value> {
        match operation {
            Operation::Submit { request } => self.submit(request).await,
            Operation::List => self.list().await,
            Operation::Get { id } => self.get(id).await,
            Operation::Answer { id, index } => self.answer(id, index).await,
        }
    }

    async fn submit(&self, request: DecisionRequest) -> Result<Value> {
        request.validate()?;
        let payload = serde_json::to_string(&request)?;

        // The unique identity makes competing retries converge on one committed row.
        sqlx::query("INSERT INTO requests (identity, payload) VALUES (?, ?) ON CONFLICT(identity) DO NOTHING")
            .bind(&request.identity).bind(&payload).execute(&self.pool).await?;
        let (id, existing_payload): (i64, String) =
            sqlx::query_as("SELECT id, payload FROM requests WHERE identity = ?")
                .bind(&request.identity)
                .fetch_one(&self.pool)
                .await?;
        if existing_payload != payload {
            return Err(AppError::IdentityConflict);
        }
        Ok(json!({"id": id}))
    }

    async fn list(&self) -> Result<Value> {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, payload FROM requests WHERE answer IS NULL ORDER BY id")
                .fetch_all(&self.pool)
                .await?;
        let requests: Vec<PendingRequest> = rows
            .into_iter()
            .map(|(id, payload)| {
                Ok(PendingRequest {
                    id,
                    request: serde_json::from_str(&payload)?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(json!({"requests": requests}))
    }

    async fn get(&self, id: i64) -> Result<Value> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT answer FROM requests WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let answer = row
            .ok_or(AppError::RequestNotFound)?
            .0
            .map(|answer| serde_json::from_str::<Value>(&answer))
            .transpose()?;
        Ok(json!({"id": id, "answer": answer}))
    }

    async fn answer(&self, id: i64, index: usize) -> Result<Value> {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT payload, answer FROM requests WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let (payload, existing_answer) = row.ok_or(AppError::RequestNotFound)?;
        if existing_answer.is_some() {
            return Err(AppError::AlreadyAnswered);
        }
        let request: DecisionRequest = serde_json::from_str(&payload)?;
        let value = request
            .options
            .get(index)
            .ok_or(AppError::InvalidOption)?
            .clone();
        let answer = serde_json::to_string(&SuggestedAnswer {
            answer_type: AnswerType::Suggested,
            index,
            value,
        })?;

        // The conditional write is atomic even when multiple clients answer at once.
        let result = sqlx::query("UPDATE requests SET answer = ? WHERE id = ? AND answer IS NULL")
            .bind(answer)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() != 1 {
            return Err(AppError::AlreadyAnswered);
        }
        Ok(json!({"id": id}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Operation;

    #[tokio::test]
    async fn opens_existing_prototype_database(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join(format!("aviary-legacy-{}", std::process::id()));
        std::fs::create_dir_all(&directory)?;
        let database_path = directory.join("legacy.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);
        let legacy_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::query("CREATE TABLE requests (id INTEGER PRIMARY KEY, identity TEXT NOT NULL UNIQUE, payload TEXT NOT NULL, answer TEXT)")
            .execute(&legacy_pool).await?;
        let payload = r#"{"identity":"legacy","agent":"agent","question":"Proceed?","options":["Yes"],"recommendation":0,"reason":"Ready"}"#;
        sqlx::query(
            "INSERT INTO requests (id, identity, payload, answer) VALUES (7, 'legacy', ?, NULL)",
        )
        .bind(payload)
        .execute(&legacy_pool)
        .await?;
        legacy_pool.close().await;

        let storage =
            Storage::open(database_path.to_str().ok_or("database path is not UTF-8")?).await?;
        let requests = storage.handle(Operation::List).await?;
        assert_eq!(requests["requests"][0]["id"], 7);
        storage
            .handle(Operation::Answer { id: 7, index: 0 })
            .await?;
        let response = storage.handle(Operation::Get { id: 7 }).await?;
        assert_eq!(response["answer"]["value"], "Yes");
        storage.pool.close().await;
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }
}
