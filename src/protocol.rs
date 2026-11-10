use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

pub const MAX_MESSAGE_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequest {
    pub identity: String,
    pub agent: String,
    pub question: String,
    pub options: Vec<String>,
    pub recommendation: usize,
    pub reason: String,
}

impl DecisionRequest {
    pub fn validate(&self) -> Result<()> {
        if self.identity.is_empty()
            || self.agent.is_empty()
            || self.question.is_empty()
            || self.reason.is_empty()
            || self.options.is_empty()
            || self.options.iter().any(String::is_empty)
            || self.recommendation >= self.options.len()
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
    Get { id: i64 },
    Answer { id: i64, index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingRequest {
    pub id: i64,
    pub request: DecisionRequest,
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
}
