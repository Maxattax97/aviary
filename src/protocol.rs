use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

pub const MAX_MESSAGE_BYTES: u64 = 1_048_576;
pub const MAX_RESPONSE_BYTES: u64 = 3 * MAX_MESSAGE_BYTES;
fn is_zero(value: &u8) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequest {
    pub identity: String,
    pub agent: String,
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
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
            || self.activity.as_deref() == Some("")
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
    Submit {
        request: DecisionRequest,
    },
    List,
    ListDeferred,
    ListPage {
        #[serde(default)]
        deferred: bool,
        #[serde(default)]
        offset: usize,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        include_newest: bool,
    },
    DecisionQueue {
        #[serde(default)]
        offset: usize,
    },
    DeferDecision {
        id: i64,
        identity: String,
    },
    SwapDecisions {
        first: i64,
        first_version: i64,
        second: i64,
        second_version: i64,
        identity: String,
    },
    DecisionStatus {
        id: i64,
        #[serde(default)]
        after: i64,
    },
    DecisionEvents {
        agent: String,
        #[serde(default)]
        after: i64,
    },
    NoticesPage {
        #[serde(default)]
        include_dismissed: bool,
        #[serde(default)]
        offset: usize,
    },
    SetImportance {
        id: i64,
        importance: u8,
    },
    SetDeferred {
        id: i64,
        deferred: bool,
    },
    Get {
        id: i64,
    },
    Answer {
        id: i64,
        index: usize,
    },
    AnswerCustom {
        id: i64,
        value: String,
    },
    CreateChannel {
        channel: Channel,
    },
    SendMessage {
        message: Message,
    },
    Inbox {
        participant: String,
    },
    InboxPage {
        participant: String,
        #[serde(default)]
        offset: usize,
    },
    ReplyMessage {
        reply: MessageReply,
    },
    GetMessageResponse {
        id: i64,
    },
    SubmitNotice {
        notice: CompletionNotice,
    },
    ListNotices {
        #[serde(default)]
        include_dismissed: bool,
    },
    DismissNotice {
        id: i64,
    },
    History {
        kind: HistoryKind,
        id: i64,
    },
    HistoryPage {
        kind: HistoryKind,
        id: i64,
        section: HistorySection,
        #[serde(default)]
        offset: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingRequest {
    pub id: i64,
    pub request: DecisionRequest,
    pub importance: u8,
    pub deferred: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CompletionNotice {
    pub identity: String,
    pub agent: String,
    pub work: String,
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboxItem {
    pub id: i64,
    pub message: Message,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InboxPage {
    pub messages: Vec<InboxItem>,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoticeItem {
    pub id: i64,
    pub notice: CompletionNotice,
    pub dismissed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    Decision,
    Message,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestPage {
    pub requests: Vec<PendingRequest>,
    pub next_offset: Option<usize>,
    #[serde(default)]
    pub newest_id: Option<i64>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct NoticePage {
    pub notices: Vec<NoticeItem>,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistorySection {
    Decisions,
    Messages,
    Outcomes,
}
