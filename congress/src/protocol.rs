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
        /// Challenge nonce from VoteChallenge message
        challenge_nonce: String,
        /// SHA256(nonce + voter_token)[0:16] computed by client
        challenge_response: String,
        /// True if navigator.webdriver is set (automation detection)
        is_webdriver: bool,
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
    /// Clear all prompts from the pool (for fresh start)
    HostClearPromptPool,
    HostAddPrompt {
        text: Option<String>,
        image_url: Option<String>,
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
    /// Remove an existing submission (host only)
    HostRemoveSubmission {
        submission_id: SubmissionId,
    },
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
    /// Shadowban all submitters of a prompt (host only, for spam prompts)
    HostShadowbanPromptSubmitters {
        prompt_id: PromptId,
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
    /// Queue a prompt for the next round (host only, max 3)
    HostQueuePrompt {
        prompt_id: PromptId,
    },
    /// Unqueue a prompt (move back to pool)
    HostUnqueuePrompt {
        prompt_id: PromptId,
    },
    /// Delete a prompt from pool or queue (host only)
    HostDeletePrompt {
        prompt_id: PromptId,
    },
    /// Vote for a prompt during PROMPT_SELECTION phase (audience)
    PromptVote {
        voter_token: String,
        prompt_id: PromptId,
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
        /// Current prompt (included when transitioning to WRITING so clients always have it)
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<Prompt>,
    },
    Submissions {
        list: Vec<SubmissionInfo>,
    },
    /// Beamer-only: number of submissions collected so far (without revealing the texts)
    SubmissionCount {
        count: u32,
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
        /// Current prompt (included during WRITING phase for state recovery)
        #[serde(skip_serializing_if = "Option::is_none")]
        current_prompt: Option<Prompt>,
    },
    /// Sent to audience on reconnect with their current vote
    AudienceState {
        /// Auto-generated friendly display name for this audience member
        display_name: String,
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
        stats: PromptPoolStats,
    },
    /// Broadcast when a player is removed from the game
    PlayerRemoved {
        player_id: PlayerId,
    },
    /// Queued prompts sent to host (prompts ready for PROMPT_SELECTION)
    HostQueuedPrompts {
        prompts: Vec<HostPromptInfo>,
    },
    /// Prompt candidates for voting (sent to all during PROMPT_SELECTION)
    PromptCandidates {
        prompts: Vec<Prompt>,
    },
    /// Prompt vote counts for beamer during PROMPT_SELECTION
    BeamerPromptVoteCounts {
        counts: HashMap<PromptId, u32>,
    },
    /// Acknowledge a prompt vote
    PromptVoteAck,
    /// Sent to audience on reconnect with their prompt vote state during PROMPT_SELECTION
    AudiencePromptVoteState {
        has_voted: bool,
        voted_prompt_id: Option<PromptId>,
    },
    /// Challenge for vote anti-automation (sent when entering VOTING phase)
    VoteChallenge {
        /// Random nonce that changes each voting round
        nonce: String,
        /// Round ID for validation
        round_id: RoundId,
    },
    /// Connection stats sent to host (periodic updates)
    HostConnectionStats {
        players: u32,
        audience: u32,
        beamers: u32,
        hosts: u32,
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

/// Prompt info sent to host (includes submitter IDs for shadowban functionality)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPromptInfo {
    pub id: PromptId,
    pub text: Option<String>,
    pub image_url: Option<String>,
    pub source: PromptSource,
    /// All submitter IDs (for deduplicated prompts, may have multiple)
    pub submitter_ids: Vec<VoterId>,
    /// How many times this prompt was submitted
    pub submission_count: u32,
    /// When this prompt was first created (ISO8601)
    pub created_at: Option<String>,
}

/// Statistics about the prompt pool (sent to host)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPoolStats {
    /// Total number of prompts in pool
    pub total: usize,
    /// Number of host-submitted prompts
    pub host_count: usize,
    /// Number of audience-submitted prompts
    pub audience_count: usize,
    /// Top submitters by prompt count (for moderation)
    pub top_submitters: Vec<SubmitterStats>,
}

/// Stats about a single submitter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitterStats {
    pub voter_id: VoterId,
    pub count: usize,
}
