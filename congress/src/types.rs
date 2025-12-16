use serde::{Deserialize, Serialize};

/// Opaque ID types for type safety
pub type GameId = String;
pub type RoundId = String;
pub type SubmissionId = String;
pub type VoteId = String;
pub type PlayerId = String;
pub type VoterId = String;
pub type PromptId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GamePhase {
    Lobby,
    PromptSelection,
    Writing,
    Reveal,
    Voting,
    Results,
    Podium,
    Intermission,
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoundState {
    Setup,
    Collecting,
    Revealing,
    OpenForVotes,
    Scoring,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub writing_seconds: u32,
    pub voting_seconds: u32,
    pub max_answer_chars: usize,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            writing_seconds: 60,
            voting_seconds: 30,
            max_answer_chars: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub version: u64,
    pub phase: GamePhase,
    pub round_no: u32,
    pub config: GameConfig,
    pub current_round_id: Option<RoundId>,
    pub phase_deadline: Option<String>, // ISO timestamp for phase timer (Writing, Voting, etc.)
    pub panic_mode: bool,               // When true, audience interactions are disabled
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub id: PromptId,
    pub text: Option<String>,
    pub image_url: Option<String>,
    pub source: PromptSource,
    /// IDs of audience members who submitted this prompt (for deduplicated prompts)
    /// Empty for host-submitted prompts
    #[serde(default)]
    pub submitter_ids: Vec<VoterId>,
    /// How many times this prompt was submitted (1 = unique, >1 = deduplicated)
    #[serde(default = "default_submission_count")]
    pub submission_count: u32,
    /// When this prompt was first created (ISO8601 timestamp)
    #[serde(default)]
    pub created_at: Option<String>,
}

fn default_submission_count() -> u32 {
    1
}

impl Prompt {
    /// Get the first/primary submitter ID (for backwards compatibility)
    pub fn submitter_id(&self) -> Option<&VoterId> {
        self.submitter_ids.first()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PromptSource {
    Host,
    Audience,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Round {
    pub id: RoundId,
    pub game_id: GameId,
    pub number: u32,
    pub state: RoundState,
    // prompt_candidates removed - now lives in AppState.prompt_pool
    pub selected_prompt: Option<Prompt>,
    pub submission_deadline: Option<String>,
    pub reveal_order: Vec<SubmissionId>,
    pub reveal_index: usize, // Current position in reveal carousel (0-based)
    pub ai_submission_id: Option<SubmissionId>,
    pub scored_at: Option<String>, // Timestamp when scores were computed (for idempotency)
    // Panic mode manual winners (host picks when audience voting is disabled)
    pub manual_ai_winner: Option<SubmissionId>,
    pub manual_funny_winner: Option<SubmissionId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthorKind {
    Player,
    Ai,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub id: SubmissionId,
    pub round_id: RoundId,
    pub author_kind: AuthorKind,
    pub author_ref: Option<PlayerId>,
    pub original_text: String,
    pub display_text: String,
    pub edited_by_host: Option<bool>,
    pub tts_asset_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub id: VoteId,
    pub round_id: RoundId,
    pub voter_id: VoterId,
    pub ai_pick_submission_id: SubmissionId,
    pub funny_pick_submission_id: SubmissionId,
    pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScoreKind {
    Player,
    Audience,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Score {
    pub id: String,
    pub kind: ScoreKind,
    pub ref_id: String,
    pub display_name: Option<String>,
    pub ai_detect_points: u32,
    pub funny_points: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub token: String,
    pub display_name: Option<String>,
}

/// Audience member with auto-generated friendly display name
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudienceMember {
    pub voter_id: VoterId,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Host,
    Beamer,
    Player,
    Audience,
}

/// Connection counts by role
#[derive(Debug, Clone, Default)]
pub struct ConnectionCounts {
    pub players: u32,
    pub audience: u32,
    pub beamers: u32,
    pub hosts: u32,
}
