//! Audience message handlers
//!
//! Handlers for audience-specific messages like voting and prompt submission.

use crate::protocol::ServerMessage;
use crate::state::vote::VoteResult;
use crate::state::AppState;
use std::sync::Arc;

pub async fn handle_vote(
    state: &Arc<AppState>,
    voter_token: String,
    ai: String,
    funny: String,
    msg_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Vote: AI={}, Funny={}, MsgID={}", ai, funny, msg_id);

    match state
        .submit_vote(voter_token, ai, funny, msg_id.clone())
        .await
    {
        VoteResult::Recorded => {
            tracing::debug!("Vote recorded");
            Some(ServerMessage::VoteAck { msg_id })
        }
        VoteResult::Duplicate => {
            tracing::debug!("Duplicate vote msg_id, returning ack");
            Some(ServerMessage::VoteAck { msg_id })
        }
        VoteResult::NoActiveRound => {
            tracing::warn!("Vote received but no active round");
            Some(ServerMessage::VoteAck { msg_id })
        }
        VoteResult::PanicModeActive => {
            tracing::info!("Vote rejected: panic mode active");
            Some(ServerMessage::Error {
                code: "PANIC_MODE".to_string(),
                msg: "Voting is temporarily disabled".to_string(),
            })
        }
    }
}

pub async fn handle_submit_prompt(
    state: &Arc<AppState>,
    voter_token: Option<String>,
    text: String,
) -> Option<ServerMessage> {
    tracing::info!("Prompt submitted: {}", text);
    let round = state.get_current_round().await?;

    // Check if this voter is shadowbanned
    if let Some(ref token) = voter_token {
        if state.is_shadowbanned(token).await {
            tracing::info!(
                "Shadowbanned voter {} submitted prompt, silently ignoring",
                token
            );
            // Return success to the user so they don't know they're shadowbanned
            return None;
        }
    }

    match state
        .add_prompt(
            &round.id,
            Some(text),
            None, // Audience prompts don't support images
            crate::types::PromptSource::Audience,
            voter_token.clone(),
        )
        .await
    {
        Ok(prompt) => {
            state.broadcast_prompts_to_host(&round.id).await;
            tracing::info!("Prompt added: {}", prompt.id);
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_FAILED".to_string(),
            msg: e,
        }),
    }
}
