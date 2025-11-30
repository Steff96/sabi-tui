//! Unified AI client wrapper

use crate::config::{Config, Provider};
use crate::gemini::{GeminiClient, GeminiError};
use crate::message::Message;
use crate::openai::{OpenAIClient, OpenAIError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AIError {
    #[error("{0}")]
    Gemini(#[from] GeminiError),
    #[error("{0}")]
    OpenAI(#[from] OpenAIError),
}

#[derive(Clone)]
pub enum AIClient {
    Gemini(GeminiClient),
    OpenAI(OpenAIClient),
}

impl AIClient {
    pub fn new(config: &Config) -> Result<Self, AIError> {
        match config.provider {
            Provider::Gemini => Ok(AIClient::Gemini(GeminiClient::new(config)?)),
            Provider::OpenAI => Ok(AIClient::OpenAI(OpenAIClient::new(config)?)),
        }
    }

    pub async fn chat(&self, messages: &[Message]) -> Result<String, AIError> {
        match self {
            AIClient::Gemini(c) => Ok(c.chat(messages).await?),
            AIClient::OpenAI(c) => Ok(c.chat(messages).await?),
        }
    }

    pub fn set_model(&mut self, model: String) {
        match self {
            AIClient::Gemini(c) => c.set_model(model),
            AIClient::OpenAI(c) => c.set_model(model),
        }
    }

    pub fn model(&self) -> &str {
        match self {
            AIClient::Gemini(c) => c.model(),
            AIClient::OpenAI(c) => c.model(),
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>, AIError> {
        match self {
            AIClient::Gemini(c) => Ok(c.list_models().await?),
            AIClient::OpenAI(_) => Ok(vec![]), // OpenAI doesn't have easy model listing
        }
    }
}
