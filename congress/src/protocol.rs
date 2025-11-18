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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        protocol: String,
        role: Role,
        game: Game,
    },
    Phase {
        phase: GamePhase,
        round_no: u32,
        server_now: String,
        deadline: Option<String>,
    },
    Submissions {
        list: Vec<SubmissionInfo>,
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
    },
    Error {
        code: String,
        msg: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionInfo {
    pub id: SubmissionId,
    pub display_text: String,
    pub author_kind: AuthorKind,
}

impl From<&Submission> for SubmissionInfo {
    fn from(s: &Submission) -> Self {
        Self {
            id: s.id.clone(),
            display_text: s.display_text.clone(),
            author_kind: s.author_kind.clone(),
        }
    }
}

/// Player token info sent to host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerToken {
    pub id: PlayerId,
    pub token: String,
}
