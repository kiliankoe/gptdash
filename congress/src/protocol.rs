use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMessage {
    Join {
        room_token: String,
    },
    RegisterPlayer {
        player_token: String,
        display_name: String,
    },
    SubmitAnswer {
        player_token: Option<String>,
        text: String,
    },
    Vote {
        voter_token: String,
        ai: SubmissionId,
        funny: SubmissionId,
        msg_id: String,
    },
    SubmitPrompt {
        voter_token: Option<String>,
        text: String,
    },
    AckNeeded {
        last_seen_server_seq: u64,
    },
    // Host-only messages
    HostCreatePlayers {
        count: u32,
    },
    HostTransitionPhase {
        phase: GamePhase,
    },
    HostStartRound,
    HostSelectPrompt {
        prompt_id: PromptId,
    },
    HostEditSubmission {
        submission_id: SubmissionId,
        new_text: String,
    },
    HostSetRevealOrder {
        order: Vec<SubmissionId>,
    },
    HostSetAiSubmission {
        submission_id: SubmissionId,
    },
    HostRevealNext,
    HostRevealPrev,
    HostResetGame,
    HostAddPrompt {
        text: String,
    },
    HostTogglePanicMode {
        enabled: bool,
    },
    HostSetManualWinner {
        winner_type: ManualWinnerType,
        submission_id: SubmissionId,
    },
    HostMarkDuplicate {
        submission_id: SubmissionId,
    },
    HostExtendTimer {
        seconds: u32,
    },
    /// Regenerate AI submissions (retry after failure or get new options)
    HostRegenerateAi,
    /// Manually write an AI submission (host override)
    HostWriteAiSubmission {
        text: String,
    },
    /// Request typo correction for text before final submission
    RequestTypoCheck {
        player_token: String,
        text: String,
    },
    /// Shadowban an audience member (host only)
    HostShadowbanAudience {
        voter_id: VoterId,
    },
    /// Remove a player from the game (host only)
    HostRemovePlayer {
        player_id: PlayerId,
    },
    /// Update an existing submission with corrected text (after typo check)
    UpdateSubmission {
        player_token: String,
        submission_id: SubmissionId,
        new_text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ManualWinnerType {
    Ai,
    Funny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        protocol: String,
        role: Role,
        game: Game,
        server_now: String,
        valid_transitions: Vec<GamePhase>,
    },
    Phase {
        phase: GamePhase,
        round_no: u32,
        server_now: String,
        deadline: Option<String>,
        valid_transitions: Vec<GamePhase>,
    },
    Submissions {
        list: Vec<SubmissionInfo>,
    },
    HostSubmissions {
        list: Vec<HostSubmissionInfo>,
    },
    VoteAck {
        msg_id: String,
    },
    BeamerVoteCounts {
        ai: HashMap<SubmissionId, u32>,
        funny: HashMap<SubmissionId, u32>,
        seq: u64,
    },
    Scores {
        players: Vec<Score>,
        audience_top: Vec<Score>,
    },
    PlayersCreated {
        players: Vec<PlayerToken>,
    },
    PlayerRegistered {
        player_id: PlayerId,
        display_name: String,
    },
    RoundStarted {
        round: Round,
    },
    PromptSelected {
        prompt: Prompt,
    },
    GameState {
        game: Game,
        valid_transitions: Vec<GamePhase>,
    },
    RevealUpdate {
        reveal_index: usize,
        submission: Option<SubmissionInfo>,
    },
    /// Sent to players on reconnect with their current state
    PlayerState {
        player_id: PlayerId,
        display_name: Option<String>,
        has_submitted: bool,
        current_submission: Option<SubmissionInfo>,
    },
    /// Sent to audience on reconnect with their current vote
    AudienceState {
        has_voted: bool,
        current_vote: Option<AudienceVoteInfo>,
    },
    /// Broadcast when panic mode is toggled
    PanicModeUpdate {
        enabled: bool,
    },
    /// Sent to a player when their submission is accepted
    SubmissionConfirmed,
    /// Sent to a player when their submission is rejected (e.g., marked as duplicate by host)
    SubmissionRejected {
        player_id: PlayerId,
        reason: String,
    },
    /// Broadcast when the deadline is extended
    DeadlineUpdate {
        deadline: String,
        server_now: String,
    },
    /// Result of typo check - sent to requesting player
    TypoCheckResult {
        original: String,
        corrected: String,
        has_changes: bool,
    },
    /// Player status list sent to host (names + submission status)
    HostPlayerStatus {
        players: Vec<PlayerStatusInfo>,
    },
    /// AI generation status update (sent to host only)
    AiGenerationStatus {
        status: AiGenStatus,
        provider: Option<String>,
        message: Option<String>,
    },
    /// Prompt candidates sent to host (includes submitter info for moderation)
    HostPrompts {
        prompts: Vec<HostPromptInfo>,
    },
    /// Broadcast when a player is removed from the game
    PlayerRemoved {
        player_id: PlayerId,
    },
    Error {
        code: String,
        msg: String,
    },
}

/// Status of AI generation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AiGenStatus {
    /// Generation started
    Started,
    /// A provider completed successfully
    ProviderSuccess,
    /// A provider failed
    ProviderFailed,
    /// All providers completed
    Completed,
    /// All providers failed
    AllFailed,
}

/// Audience vote info for state recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudienceVoteInfo {
    pub ai_pick: SubmissionId,
    pub funny_pick: SubmissionId,
}

/// Public submission info (no author_kind to prevent spoilers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionInfo {
    pub id: SubmissionId,
    pub display_text: String,
}

impl From<&Submission> for SubmissionInfo {
    fn from(s: &Submission) -> Self {
        Self {
            id: s.id.clone(),
            display_text: s.display_text.clone(),
        }
    }
}

/// Host-only submission info (includes author_kind and provider metadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSubmissionInfo {
    pub id: SubmissionId,
    pub display_text: String,
    pub author_kind: AuthorKind,
    pub author_ref: Option<String>, // For AI: "openai:gpt-4o-mini", for players: player ID
}

impl From<&Submission> for HostSubmissionInfo {
    fn from(s: &Submission) -> Self {
        Self {
            id: s.id.clone(),
            display_text: s.display_text.clone(),
            author_kind: s.author_kind.clone(),
            author_ref: s.author_ref.clone(),
        }
    }
}

/// Player token info sent to host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerToken {
    pub id: PlayerId,
    pub token: String,
}

/// Player submission status for host display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PlayerSubmissionStatus {
    /// Player hasn't submitted yet
    NotSubmitted,
    /// Player has submitted their answer
    Submitted,
    /// Player is waiting for typo check result
    CheckingTypos,
}

/// Player status info sent to host (combines token, name, and submission status)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStatusInfo {
    pub id: PlayerId,
    pub token: String,
    pub display_name: Option<String>,
    pub status: PlayerSubmissionStatus,
}

/// Prompt info sent to host (includes submitter ID for shadowban functionality)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPromptInfo {
    pub id: PromptId,
    pub text: Option<String>,
    pub source: PromptSource,
    pub submitter_id: Option<VoterId>,
}
