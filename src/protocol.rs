use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

pub const MAX_MESSAGE_BYTES: u64 = 1_048_576;
fn is_zero(value: &u8) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequest {
    pub identity: String,
    pub agent: String,
    pub question: String,
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub importance: u8,
    #[serde(default)]
    pub recommendation: Option<usize>,
    #[serde(default)]
    pub reason: Option<String>,
}

impl DecisionRequest {
    pub fn validate(&self) -> Result<()> {
        if self.identity.is_empty()
            || self.agent.is_empty()
            || self.question.is_empty()
            || self.options.iter().any(String::is_empty)
            || self.importance > 5
            || self
                .recommendation
                .is_some_and(|index| index >= self.options.len())
            || self.recommendation.is_some() && self.reason.as_deref().is_none_or(str::is_empty)
            || self.recommendation.is_none() && self.reason.is_some()
        {
            return Err(AppError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    Submit { request: DecisionRequest },
    List,
    ListDeferred,
    SetImportance { id: i64, importance: u8 },
    SetDeferred { id: i64, deferred: bool },
    Get { id: i64 },
    Answer { id: i64, index: usize },
    AnswerCustom { id: i64, value: String },
    CreateChannel { channel: Channel },
    SendMessage { message: Message },
    Inbox { participant: String },
    ReplyMessage { reply: MessageReply },
    GetMessageResponse { id: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingRequest {
    pub id: i64,
    pub request: DecisionRequest,
    pub importance: u8,
    pub deferred: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestList {
    pub requests: Vec<PendingRequest>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestedAnswer {
    #[serde(rename = "type")]
    pub answer_type: AnswerType,
    pub index: usize,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerType {
    Suggested,
    Custom,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CustomAnswer {
    #[serde(rename = "type")]
    pub answer_type: AnswerType,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub name: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Participant { name: String },
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub identity: String,
    pub channel: String,
    pub sender: String,
    pub target: Target,
    pub body: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub expects_reply: bool,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MessageReply {
    pub identity: String,
    pub request_id: i64,
    pub sender: String,
    pub body: String,
}
