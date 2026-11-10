use std::time::Duration;

use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};

use crate::{
    error::{AppError, Result},
    protocol::{
        AnswerType, Channel, CompletionNotice, CustomAnswer, DecisionRequest, HistoryKind, Message,
        MessageReply, Operation, PendingRequest, SuggestedAnswer, Target,
    },
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
        sqlx::query("CREATE TABLE IF NOT EXISTS submission_keys (identity TEXT PRIMARY KEY, kind TEXT NOT NULL, payload TEXT NOT NULL, record_id INTEGER)").execute(&pool).await?;
        sqlx::query("INSERT OR IGNORE INTO submission_keys (identity, kind, payload, record_id) SELECT identity, 'decision', payload, id FROM requests").execute(&pool).await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS channels (name TEXT PRIMARY KEY, members TEXT NOT NULL)",
        )
        .execute(&pool)
        .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS messages (id INTEGER PRIMARY KEY, identity TEXT NOT NULL UNIQUE, payload TEXT NOT NULL)").execute(&pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS message_recipients (message_id INTEGER NOT NULL, participant TEXT NOT NULL, PRIMARY KEY (message_id, participant), FOREIGN KEY (message_id) REFERENCES messages(id))").execute(&pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS message_responses (request_id INTEGER PRIMARY KEY, response_id INTEGER NOT NULL UNIQUE, FOREIGN KEY (request_id) REFERENCES messages(id), FOREIGN KEY (response_id) REFERENCES messages(id))").execute(&pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS request_attention (request_id INTEGER PRIMARY KEY, importance INTEGER, deferred INTEGER NOT NULL DEFAULT 0, FOREIGN KEY (request_id) REFERENCES requests(id))").execute(&pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS completion_notices (id INTEGER PRIMARY KEY, identity TEXT NOT NULL UNIQUE, payload TEXT NOT NULL, dismissed INTEGER NOT NULL DEFAULT 0)").execute(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn handle(&self, operation: Operation) -> Result<Value> {
        match operation {
            Operation::Submit { request } => self.submit(request).await,
            Operation::List => self.list(false).await,
            Operation::ListDeferred => self.list(true).await,
            Operation::SetImportance { id, importance } => {
                self.set_importance(id, importance).await
            }
            Operation::SetDeferred { id, deferred } => self.set_deferred(id, deferred).await,
            Operation::Get { id } => self.get(id).await,
            Operation::Answer { id, index } => self.answer(id, index).await,
            Operation::AnswerCustom { id, value } => self.answer_custom(id, value).await,
            Operation::CreateChannel { channel } => self.create_channel(channel).await,
            Operation::SendMessage { message } => self.send_message(message).await,
            Operation::Inbox { participant } => self.inbox(participant).await,
            Operation::ReplyMessage { reply } => self.reply_message(reply).await,
            Operation::GetMessageResponse { id } => self.get_message_response(id).await,
            Operation::SubmitNotice { notice } => self.submit_notice(notice).await,
            Operation::ListNotices { include_dismissed } => {
                self.list_notices(include_dismissed).await
            }
            Operation::DismissNotice { id } => self.dismiss_notice(id).await,
            Operation::History { kind, id } => self.history(kind, id).await,
        }
    }

    async fn submit(&self, request: DecisionRequest) -> Result<Value> {
        request.validate()?;
        let payload = serde_json::to_string(&request)?;

        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO submission_keys (identity, kind, payload) VALUES (?, 'decision', ?) ON CONFLICT(identity) DO NOTHING")
            .bind(&request.identity).bind(&payload).execute(&mut *tx).await?.rows_affected() == 1;
        let id = if inserted {
            let result = sqlx::query("INSERT INTO requests (identity, payload) VALUES (?, ?)")
                .bind(&request.identity)
                .bind(&payload)
                .execute(&mut *tx)
                .await?;
            let id = result.last_insert_rowid();
            sqlx::query("UPDATE submission_keys SET record_id = ? WHERE identity = ?")
                .bind(id)
                .bind(&request.identity)
                .execute(&mut *tx)
                .await?;
            id
        } else {
            Self::retry_id(&mut tx, &request.identity, "decision", &payload).await?
        };
        tx.commit().await?;
        Ok(json!({"id": id}))
    }

    async fn retry_id(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        identity: &str,
        kind: &str,
        payload: &str,
    ) -> Result<i64> {
        let (existing_kind, existing_payload, id): (String, String, Option<i64>) = sqlx::query_as(
            "SELECT kind, payload, record_id FROM submission_keys WHERE identity = ?",
        )
        .bind(identity)
        .fetch_one(&mut **tx)
        .await?;
        if existing_kind != kind || existing_payload != payload {
            return Err(AppError::IdentityConflict);
        }
        id.ok_or(AppError::IdentityConflict)
    }

    async fn submit_notice(&self, notice: CompletionNotice) -> Result<Value> {
        if notice.identity.is_empty()
            || notice.agent.is_empty()
            || notice.work.is_empty()
            || notice.result.is_empty()
        {
            return Err(AppError::InvalidRequest);
        }
        let payload = serde_json::to_string(&notice)?;
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO submission_keys (identity, kind, payload) VALUES (?, 'notice', ?) ON CONFLICT(identity) DO NOTHING")
            .bind(&notice.identity).bind(&payload).execute(&mut *tx).await?.rows_affected() == 1;
        let id = if inserted {
            let id =
                sqlx::query("INSERT INTO completion_notices (identity, payload) VALUES (?, ?)")
                    .bind(&notice.identity)
                    .bind(&payload)
                    .execute(&mut *tx)
                    .await?
                    .last_insert_rowid();
            sqlx::query("UPDATE submission_keys SET record_id = ? WHERE identity = ?")
                .bind(id)
                .bind(&notice.identity)
                .execute(&mut *tx)
                .await?;
            id
        } else {
            Self::retry_id(&mut tx, &notice.identity, "notice", &payload).await?
        };
        tx.commit().await?;
        Ok(json!({"id": id}))
    }

    async fn list_notices(&self, include_dismissed: bool) -> Result<Value> {
        let rows: Vec<(i64, String, bool)> = sqlx::query_as("SELECT id, payload, dismissed FROM completion_notices WHERE ? OR dismissed = 0 ORDER BY id")
            .bind(include_dismissed).fetch_all(&self.pool).await?;
        let notices = rows.into_iter().map(|(id, payload, dismissed)| -> Result<Value> { Ok(json!({"id":id, "notice":serde_json::from_str::<CompletionNotice>(&payload)?, "dismissed":dismissed})) }).collect::<Result<Vec<_>>>()?;
        Ok(json!({"notices":notices}))
    }

    async fn dismiss_notice(&self, id: i64) -> Result<Value> {
        let result = sqlx::query("UPDATE completion_notices SET dismissed = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::RequestNotFound);
        }
        Ok(json!({"id":id, "dismissed":true}))
    }

    async fn create_channel(&self, mut channel: Channel) -> Result<Value> {
        if channel.name.is_empty()
            || channel.members.is_empty()
            || channel.members.iter().any(String::is_empty)
        {
            return Err(AppError::InvalidRequest);
        }
        channel.members.sort();
        if channel
            .members
            .windows(2)
            .any(|pair| pair.first() == pair.get(1))
        {
            return Err(AppError::InvalidRequest);
        }
        let members = serde_json::to_string(&channel.members)?;
        sqlx::query(
            "INSERT INTO channels (name, members) VALUES (?, ?) ON CONFLICT(name) DO NOTHING",
        )
        .bind(&channel.name)
        .bind(&members)
        .execute(&self.pool)
        .await?;
        let existing: String = sqlx::query_scalar("SELECT members FROM channels WHERE name = ?")
            .bind(&channel.name)
            .fetch_one(&self.pool)
            .await?;
        if existing != members {
            return Err(AppError::IdentityConflict);
        }
        Ok(json!({"channel": channel}))
    }

    async fn send_message(&self, message: Message) -> Result<Value> {
        if message.identity.is_empty()
            || message.channel.is_empty()
            || message.sender.is_empty()
            || message.body.is_empty()
        {
            return Err(AppError::InvalidRequest);
        }
        if message.expects_reply && !matches!(message.target, Target::Participant { .. }) {
            return Err(AppError::InvalidRequest);
        }
        let payload = serde_json::to_string(&message)?;
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO submission_keys (identity, kind, payload) VALUES (?, 'message', ?) ON CONFLICT(identity) DO NOTHING")
            .bind(&message.identity).bind(&payload).execute(&mut *tx).await?.rows_affected() == 1;
        if !inserted {
            let id = Self::retry_id(&mut tx, &message.identity, "message", &payload).await?;
            tx.commit().await?;
            return Ok(json!({"id":id}));
        }
        let members: Option<String> =
            sqlx::query_scalar("SELECT members FROM channels WHERE name = ?")
                .bind(&message.channel)
                .fetch_optional(&mut *tx)
                .await?;
        let members: Vec<String> = serde_json::from_str(&members.ok_or(AppError::InvalidRequest)?)?;
        if !members.contains(&message.sender) {
            return Err(AppError::InvalidRequest);
        }
        let recipients: Vec<String> = match &message.target {
            Target::Participant { name } if members.contains(name) => vec![name.clone()],
            Target::Group => members
                .into_iter()
                .filter(|member| member != &message.sender)
                .collect(),
            _ => return Err(AppError::InvalidRequest),
        };
        if recipients.is_empty() {
            return Err(AppError::InvalidRequest);
        }
        let id = sqlx::query("INSERT INTO messages (identity, payload) VALUES (?, ?)")
            .bind(&message.identity)
            .bind(&payload)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
        for recipient in recipients {
            sqlx::query("INSERT INTO message_recipients (message_id, participant) VALUES (?, ?)")
                .bind(id)
                .bind(recipient)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE submission_keys SET record_id = ? WHERE identity = ?")
            .bind(id)
            .bind(&message.identity)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(json!({"id":id}))
    }

    async fn reply_message(&self, reply: MessageReply) -> Result<Value> {
        if reply.identity.is_empty() || reply.sender.is_empty() || reply.body.is_empty() {
            return Err(AppError::InvalidRequest);
        }
        let payload = serde_json::to_string(&reply)?;
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO submission_keys (identity, kind, payload) VALUES (?, 'response', ?) ON CONFLICT(identity) DO NOTHING")
            .bind(&reply.identity).bind(&payload).execute(&mut *tx).await?.rows_affected() == 1;
        if !inserted {
            let response_id =
                Self::retry_id(&mut tx, &reply.identity, "response", &payload).await?;
            tx.commit().await?;
            return Ok(json!({"id": response_id, "request_id": reply.request_id}));
        }
        let original: Option<String> =
            sqlx::query_scalar("SELECT payload FROM messages WHERE id = ?")
                .bind(reply.request_id)
                .fetch_optional(&mut *tx)
                .await?;
        let original: Message = serde_json::from_str(&original.ok_or(AppError::RequestNotFound)?)?;
        if !original.expects_reply
            || !matches!(&original.target, Target::Participant { name } if name == &reply.sender)
        {
            return Err(AppError::InvalidRequest);
        }
        let responded: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM message_responses WHERE request_id = ?)",
        )
        .bind(reply.request_id)
        .fetch_one(&mut *tx)
        .await?;
        if responded {
            return Err(AppError::AlreadyAnswered);
        }
        let response = Message {
            identity: reply.identity.clone(),
            channel: original.channel,
            sender: reply.sender,
            target: Target::Participant {
                name: original.sender.clone(),
            },
            body: reply.body,
            expects_reply: false,
            context: original.context,
            activity: original.activity,
        };
        let response_payload = serde_json::to_string(&response)?;
        let id = sqlx::query("INSERT INTO messages (identity, payload) VALUES (?, ?)")
            .bind(&reply.identity)
            .bind(response_payload)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
        sqlx::query("INSERT INTO message_recipients (message_id, participant) VALUES (?, ?)")
            .bind(id)
            .bind(original.sender)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO message_responses (request_id, response_id) VALUES (?, ?)")
            .bind(reply.request_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE submission_keys SET record_id = ? WHERE identity = ?")
            .bind(id)
            .bind(reply.identity)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(json!({"id": id, "request_id": reply.request_id}))
    }

    async fn get_message_response(&self, id: i64) -> Result<Value> {
        let original: Option<String> =
            sqlx::query_scalar("SELECT payload FROM messages WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let original: Message = serde_json::from_str(&original.ok_or(AppError::RequestNotFound)?)?;
        if !original.expects_reply {
            return Err(AppError::InvalidRequest);
        }
        let response: Option<(i64, String)> = sqlx::query_as("SELECT m.id, m.payload FROM message_responses r JOIN messages m ON m.id = r.response_id WHERE r.request_id = ?").bind(id).fetch_optional(&self.pool).await?;
        let response = response.map(|(response_id, payload)| -> Result<Value> { Ok(json!({"id": response_id, "message": serde_json::from_str::<Message>(&payload)?})) }).transpose()?;
        Ok(json!({"id": id, "response": response}))
    }

    async fn history(&self, kind: HistoryKind, id: i64) -> Result<Value> {
        // One read transaction gives all sections the same SQLite snapshot.
        let mut tx = self.pool.begin().await?;
        let (table, root_kind) = match kind {
            HistoryKind::Decision => ("requests", "decision"),
            HistoryKind::Message => ("messages", "message"),
        };
        let root_payload: Option<String> =
            sqlx::query_scalar(&format!("SELECT payload FROM {table} WHERE id = ?"))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let root: Value = serde_json::from_str(&root_payload.ok_or(AppError::RequestNotFound)?)?;
        let activity = root
            .get("activity")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty());
        let decisions: Vec<(i64, String, Option<String>)> = sqlx::query_as("SELECT id, payload, answer FROM requests WHERE (id = ? AND ? = 'decision') OR (? IS NOT NULL AND json_extract(payload, '$.activity') = ?) ORDER BY id")
            .bind(id).bind(root_kind).bind(activity).bind(activity).fetch_all(&mut *tx).await?;
        let decisions = decisions.into_iter().map(|(id, payload, answer)| -> Result<Value> {
            let answer = answer.map(|value| serde_json::from_str::<Value>(&value)).transpose()?;
            Ok(json!({"id":id, "request":serde_json::from_str::<Value>(&payload)?, "answer":answer}))
        }).collect::<Result<Vec<_>>>()?;
        let messages: Vec<(i64, String, Option<i64>)> = sqlx::query_as("SELECT m.id, m.payload, r.request_id FROM messages m LEFT JOIN message_responses r ON r.response_id = m.id WHERE (m.id = ? AND ? = 'message') OR (r.request_id = ? AND ? = 'message') OR (m.id = (SELECT request_id FROM message_responses WHERE response_id = ?) AND ? = 'message') OR (? IS NOT NULL AND json_extract(m.payload, '$.activity') = ?) ORDER BY m.id")
            .bind(id).bind(root_kind).bind(id).bind(root_kind).bind(id).bind(root_kind).bind(activity).bind(activity).fetch_all(&mut *tx).await?;
        let messages = messages.into_iter().map(|(id, payload, reply_to)| -> Result<Value> {
            Ok(json!({"id":id, "message":serde_json::from_str::<Value>(&payload)?, "reply_to":reply_to}))
        }).collect::<Result<Vec<_>>>()?;
        let notices: Vec<(i64, String, bool)> = sqlx::query_as("SELECT id, payload, dismissed FROM completion_notices WHERE ? IS NOT NULL AND json_extract(payload, '$.activity') = ? ORDER BY id")
            .bind(activity).bind(activity).fetch_all(&mut *tx).await?;
        let outcomes = notices.into_iter().map(|(id, payload, dismissed)| -> Result<Value> {
            Ok(json!({"id":id, "notice":serde_json::from_str::<Value>(&payload)?, "dismissed":dismissed}))
        }).collect::<Result<Vec<_>>>()?;
        tx.commit().await?;
        Ok(
            json!({"root":{"kind":root_kind,"id":id}, "activity":activity, "decisions":decisions, "messages":messages, "outcomes":outcomes}),
        )
    }

    async fn inbox(&self, participant: String) -> Result<Value> {
        if participant.is_empty() {
            return Err(AppError::InvalidRequest);
        }
        let rows: Vec<(i64, String)> = sqlx::query_as("SELECT m.id, m.payload FROM messages m JOIN message_recipients r ON r.message_id = m.id WHERE r.participant = ? ORDER BY m.id")
            .bind(participant).fetch_all(&self.pool).await?;
        let messages = rows
            .into_iter()
            .map(|(id, payload)| -> Result<Value> {
                Ok(json!({"id": id, "message": serde_json::from_str::<Message>(&payload)?}))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"messages": messages}))
    }

    async fn list(&self, deferred: bool) -> Result<Value> {
        let rows: Vec<(i64, String, Option<i64>, bool)> = sqlx::query_as("SELECT r.id, r.payload, a.importance, COALESCE(a.deferred, 0) FROM requests r LEFT JOIN request_attention a ON a.request_id = r.id WHERE r.answer IS NULL AND COALESCE(a.deferred, 0) = ?")
            .bind(deferred).fetch_all(&self.pool).await?;
        let mut requests = rows
            .into_iter()
            .map(
                |(id, payload, override_value, deferred)| -> Result<PendingRequest> {
                    let request: DecisionRequest = serde_json::from_str(&payload)?;
                    let importance = override_value
                        .map(|value| value as u8)
                        .unwrap_or(request.importance);
                    Ok(PendingRequest {
                        id,
                        request,
                        importance,
                        deferred,
                    })
                },
            )
            .collect::<Result<Vec<_>>>()?;
        requests.sort_by_key(|item| (std::cmp::Reverse(item.importance), item.id));
        Ok(json!({"requests": requests}))
    }

    async fn set_importance(&self, id: i64, importance: u8) -> Result<Value> {
        if importance > 5 {
            return Err(AppError::InvalidRequest);
        }
        let result = sqlx::query("INSERT INTO request_attention (request_id, importance) SELECT id, ? FROM requests WHERE id = ? AND answer IS NULL ON CONFLICT(request_id) DO UPDATE SET importance = excluded.importance")
            .bind(importance).bind(id).execute(&self.pool).await?;
        if result.rows_affected() == 0 {
            return Err(AppError::RequestNotFound);
        }
        Ok(json!({"id": id, "importance": importance}))
    }

    async fn set_deferred(&self, id: i64, deferred: bool) -> Result<Value> {
        let result = sqlx::query("INSERT INTO request_attention (request_id, deferred) SELECT id, ? FROM requests WHERE id = ? AND answer IS NULL ON CONFLICT(request_id) DO UPDATE SET deferred = excluded.deferred")
            .bind(deferred).bind(id).execute(&self.pool).await?;
        if result.rows_affected() == 0 {
            return Err(AppError::RequestNotFound);
        }
        Ok(json!({"id": id, "deferred": deferred}))
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

        self.store_answer(id, answer).await
    }

    async fn answer_custom(&self, id: i64, value: String) -> Result<Value> {
        if value.is_empty() {
            return Err(AppError::InvalidOption);
        }
        let answer = serde_json::to_string(&CustomAnswer {
            answer_type: AnswerType::Custom,
            value,
        })?;
        self.store_answer(id, answer).await
    }

    async fn store_answer(&self, id: i64, answer: String) -> Result<Value> {
        // The conditional write is atomic even when multiple clients answer at once.
        let result = sqlx::query("UPDATE requests SET answer = ? WHERE id = ? AND answer IS NULL")
            .bind(answer)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() != 1 {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM requests WHERE id = ?)")
                    .bind(id)
                    .fetch_one(&self.pool)
                    .await?;
            return Err(if exists {
                AppError::AlreadyAnswered
            } else {
                AppError::RequestNotFound
            });
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
