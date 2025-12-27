pub mod export;
mod game;
mod player;
mod round;
mod score;
mod submission;
pub mod trivia;
pub mod vote;

use crate::llm::{LlmConfig, LlmManager};
use crate::protocol::{PlayerSubmissionStatus, ServerMessage};
use crate::types::*;
use export::GameStateExport;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};

/// Token bucket for per-token WebSocket message rate limiting
#[derive(Debug)]
struct WsTokenBucket {
    rate_per_sec: f64,
    burst: f64,
    allowance: f64,
    last_check: Instant,
    last_used: Instant,
}

impl WsTokenBucket {
    fn new(rate_per_sec: f64, burst: f64) -> Self {
        let now = Instant::now();
        Self {
            rate_per_sec,
            burst,
            allowance: burst,
            last_check: now,
            last_used: now,
        }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = (now - self.last_check).as_secs_f64();
        self.last_check = now;
        self.last_used = now;

        self.allowance = (self.allowance + elapsed * self.rate_per_sec).min(self.burst);
        if self.allowance < 1.0 {
            return false;
        }
        self.allowance -= 1.0;
        true
    }

    fn is_stale(&self, max_idle_secs: u64) -> bool {
        self.last_used.elapsed().as_secs() > max_idle_secs
    }
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub game: Arc<RwLock<Option<Game>>>,
    pub rounds: Arc<RwLock<HashMap<RoundId, Round>>>,
    pub submissions: Arc<RwLock<HashMap<SubmissionId, Submission>>>,
    /// Secondary index: submissions by round for O(1) lookup
    pub submissions_by_round: Arc<RwLock<HashMap<RoundId, HashSet<SubmissionId>>>>,
    pub votes: Arc<RwLock<HashMap<VoteId, Vote>>>,
    pub players: Arc<RwLock<HashMap<PlayerId, Player>>>,
    pub scores: Arc<RwLock<Vec<Score>>>,
    /// Processed vote msg_ids per voter for idempotency (voter_id -> msg_id)
    pub processed_vote_msg_ids: Arc<RwLock<HashMap<VoterId, String>>>,
    /// Player submission status tracking (player_id -> status)
    pub player_status: Arc<RwLock<HashMap<PlayerId, PlayerSubmissionStatus>>>,
    /// Shadowbanned audience member IDs (their prompts are silently ignored)
    pub shadowbanned_audience: Arc<RwLock<HashSet<VoterId>>>,
    /// Global prompt pool - persists across rounds and game resets
    pub prompt_pool: Arc<RwLock<HashMap<PromptId, Prompt>>>,
    /// Flag to indicate prompt pool has changed (for debounced host broadcasts)
    pub prompts_dirty: Arc<AtomicBool>,
    /// Queued prompts for the next round (1-3 max, host selects from pool)
    pub queued_prompts: Arc<RwLock<Vec<Prompt>>>,
    /// Audience votes on which queued prompt to use (voter_id -> prompt_id)
    pub prompt_votes: Arc<RwLock<HashMap<VoterId, PromptId>>>,
    /// Audience members with auto-generated display names (persists across games)
    pub audience_members: Arc<RwLock<HashMap<VoterId, AudienceMember>>>,
    /// Cache of used display names for O(1) uniqueness checks (avoids O(n) cloning)
    pub used_display_names: Arc<RwLock<HashSet<String>>>,
    /// Vote challenge nonce (changes per voting round, used for anti-automation)
    pub vote_challenge_nonce: Arc<RwLock<Option<String>>>,
    /// Timestamp when VOTING phase started (for server-side timing validation)
    pub voting_phase_started_at: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
    /// Trivia questions pool (persists across rounds/games)
    pub trivia_questions: Arc<RwLock<Vec<TriviaQuestion>>>,
    /// Currently active trivia question ID (during WRITING phase)
    pub active_trivia: Arc<RwLock<Option<TriviaQuestionId>>>,
    /// Trivia votes per question (question_id -> vec of votes)
    pub trivia_votes: Arc<RwLock<HashMap<TriviaQuestionId, Vec<TriviaVote>>>>,
    /// LLM manager for generating AI answers
    pub llm: Option<Arc<LlmManager>>,
    /// LLM configuration (timeout, max tokens, etc.)
    pub llm_config: LlmConfig,
    /// Broadcast channel for sending messages to all clients
    pub broadcast: broadcast::Sender<ServerMessage>,
    /// Broadcast channel for sending messages to Host clients only
    pub host_broadcast: broadcast::Sender<ServerMessage>,
    /// Broadcast channel for sending messages to Beamer clients only
    pub beamer_broadcast: broadcast::Sender<ServerMessage>,
    /// Connected player count
    pub connected_players: Arc<AtomicU32>,
    /// Connected audience count
    pub connected_audience: Arc<AtomicU32>,
    /// Connected beamer count
    pub connected_beamers: Arc<AtomicU32>,
    /// Connected host count
    pub connected_hosts: Arc<AtomicU32>,
    /// Signal to disconnect all audience connections (for panic mode)
    pub audience_disconnect: broadcast::Sender<()>,
    /// Per-token rate limiters for WebSocket message rate limiting
    rate_limiters: Arc<RwLock<HashMap<String, WsTokenBucket>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self::new_with_llm(None, LlmConfig::default())
    }

    pub fn new_with_llm(llm: Option<LlmManager>, llm_config: LlmConfig) -> Self {
        let (broadcast_tx, _rx) = broadcast::channel(100);
        let (host_tx, _rx) = broadcast::channel(100);
        let (beamer_tx, _rx) = broadcast::channel(100);
        let (audience_disconnect_tx, _rx) = broadcast::channel(16);
        Self {
            game: Arc::new(RwLock::new(None)),
            rounds: Arc::new(RwLock::new(HashMap::new())),
            submissions: Arc::new(RwLock::new(HashMap::new())),
            submissions_by_round: Arc::new(RwLock::new(HashMap::new())),
            votes: Arc::new(RwLock::new(HashMap::new())),
            players: Arc::new(RwLock::new(HashMap::new())),
            scores: Arc::new(RwLock::new(Vec::new())),
            processed_vote_msg_ids: Arc::new(RwLock::new(HashMap::new())),
            player_status: Arc::new(RwLock::new(HashMap::new())),
            shadowbanned_audience: Arc::new(RwLock::new(HashSet::new())),
            prompt_pool: Arc::new(RwLock::new(HashMap::new())),
            prompts_dirty: Arc::new(AtomicBool::new(false)),
            queued_prompts: Arc::new(RwLock::new(Vec::new())),
            prompt_votes: Arc::new(RwLock::new(HashMap::new())),
            audience_members: Arc::new(RwLock::new(HashMap::new())),
            used_display_names: Arc::new(RwLock::new(HashSet::new())),
            vote_challenge_nonce: Arc::new(RwLock::new(None)),
            voting_phase_started_at: Arc::new(RwLock::new(None)),
            trivia_questions: Arc::new(RwLock::new(Vec::new())),
            active_trivia: Arc::new(RwLock::new(None)),
            trivia_votes: Arc::new(RwLock::new(HashMap::new())),
            llm: llm.map(Arc::new),
            llm_config,
            broadcast: broadcast_tx,
            host_broadcast: host_tx,
            beamer_broadcast: beamer_tx,
            connected_players: Arc::new(AtomicU32::new(0)),
            connected_audience: Arc::new(AtomicU32::new(0)),
            connected_beamers: Arc::new(AtomicU32::new(0)),
            connected_hosts: Arc::new(AtomicU32::new(0)),
            audience_disconnect: audience_disconnect_tx,
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Broadcast a message to all connected clients
    pub fn broadcast_to_all(&self, msg: ServerMessage) {
        let _ = self.broadcast.send(msg);
    }

    /// Broadcast a message to host clients only
    pub fn broadcast_to_host(&self, msg: ServerMessage) {
        let _ = self.host_broadcast.send(msg);
    }

    /// Broadcast a message to beamer clients only
    pub fn broadcast_to_beamer(&self, msg: ServerMessage) {
        let _ = self.beamer_broadcast.send(msg);
    }

    /// Signal all audience connections to disconnect (used when panic mode is enabled)
    pub fn disconnect_all_audience(&self) {
        let _ = self.audience_disconnect.send(());
        tracing::info!("Sent disconnect signal to all audience connections");
    }

    // =========================================================================
    // Rate Limiting
    // =========================================================================

    /// Check if a token is allowed to send a message (per-token rate limiting).
    /// Returns true if allowed, false if rate limited.
    pub async fn check_rate_limit(&self, token: &str) -> bool {
        let mut limiters = self.rate_limiters.write().await;
        let limiter = limiters
            .entry(token.to_string())
            .or_insert_with(|| WsTokenBucket::new(20.0, 40.0));
        limiter.allow()
    }

    /// Clean up stale rate limiters that haven't been used recently
    pub async fn cleanup_stale_rate_limiters(&self) {
        const MAX_IDLE_SECS: u64 = 300; // 5 minutes
        let mut limiters = self.rate_limiters.write().await;
        let before_count = limiters.len();
        limiters.retain(|_, limiter| !limiter.is_stale(MAX_IDLE_SECS));
        let removed = before_count - limiters.len();
        if removed > 0 {
            tracing::debug!("Cleaned up {} stale rate limiters", removed);
        }
    }

    // =========================================================================
    // Connection Tracking
    // =========================================================================

    /// Increment the connection count for a role
    pub fn increment_connection(&self, role: &Role) {
        match role {
            Role::Player => self.connected_players.fetch_add(1, Ordering::Relaxed),
            Role::Audience => self.connected_audience.fetch_add(1, Ordering::Relaxed),
            Role::Beamer => self.connected_beamers.fetch_add(1, Ordering::Relaxed),
            Role::Host => self.connected_hosts.fetch_add(1, Ordering::Relaxed),
        };
    }

    /// Decrement the connection count for a role
    pub fn decrement_connection(&self, role: &Role) {
        match role {
            Role::Player => self.connected_players.fetch_sub(1, Ordering::Relaxed),
            Role::Audience => self.connected_audience.fetch_sub(1, Ordering::Relaxed),
            Role::Beamer => self.connected_beamers.fetch_sub(1, Ordering::Relaxed),
            Role::Host => self.connected_hosts.fetch_sub(1, Ordering::Relaxed),
        };
    }

    /// Get current connection counts for all roles
    pub fn get_connection_counts(&self) -> ConnectionCounts {
        ConnectionCounts {
            players: self.connected_players.load(Ordering::Relaxed),
            audience: self.connected_audience.load(Ordering::Relaxed),
            beamers: self.connected_beamers.load(Ordering::Relaxed),
            hosts: self.connected_hosts.load(Ordering::Relaxed),
        }
    }

    /// Check if a voter is shadowbanned
    pub async fn is_shadowbanned(&self, voter_id: &str) -> bool {
        self.shadowbanned_audience.read().await.contains(voter_id)
    }

    /// Shadowban an audience member
    pub async fn shadowban_audience(&self, voter_id: String) {
        self.shadowbanned_audience.write().await.insert(voter_id);
    }

    /// Get all shadowbanned audience member IDs
    pub async fn get_shadowbanned_audience(&self) -> Vec<String> {
        self.shadowbanned_audience
            .read()
            .await
            .iter()
            .cloned()
            .collect()
    }

    // =========================================================================
    // Venue-Only Mode (IP filtering)
    // =========================================================================

    /// Check if an IP is allowed by venue config (hardcoded ranges for 38C3)
    pub fn is_ip_allowed_by_venue(&self, ip: std::net::IpAddr) -> bool {
        use crate::types::VENUE_ALLOWED_IP_RANGES;

        for range_str in VENUE_ALLOWED_IP_RANGES {
            if let Ok(network) = range_str.parse::<ipnet::IpNet>() {
                if network.contains(&ip) {
                    return true;
                }
            }
        }
        false
    }

    /// Get the venue rejection message
    pub fn get_venue_rejection_message(&self) -> &'static str {
        crate::types::VENUE_REJECTION_MESSAGE
    }

    // =========================================================================
    // Vote Challenge (anti-automation)
    // =========================================================================

    /// Generate a new vote challenge nonce for the current voting round
    /// Returns the generated nonce (32 hex chars)
    pub async fn generate_vote_challenge(&self) -> String {
        use rand::Rng;
        let bytes: [u8; 16] = rand::rng().random();
        let nonce = hex::encode(bytes);
        *self.vote_challenge_nonce.write().await = Some(nonce.clone());
        tracing::debug!("Generated vote challenge nonce: {}", nonce);
        nonce
    }

    /// Get the current vote challenge nonce (if any)
    pub async fn get_vote_challenge_nonce(&self) -> Option<String> {
        self.vote_challenge_nonce.read().await.clone()
    }

    /// Verify a vote challenge response
    /// Returns true if the response is valid for the given voter_token and nonce
    pub fn verify_vote_challenge(
        expected_nonce: &str,
        voter_token: &str,
        provided_nonce: &str,
        response: &str,
    ) -> bool {
        // Check nonce matches
        if expected_nonce != provided_nonce {
            tracing::debug!(
                "Challenge nonce mismatch: expected={}, provided={}",
                expected_nonce,
                provided_nonce
            );
            return false;
        }

        // Compute expected response: SHA256(nonce + voter_token)[0:16] as hex
        use sha2::{Digest, Sha256};
        let input = format!("{}{}", provided_nonce, voter_token);
        let hash = Sha256::digest(input.as_bytes());
        let expected_response = hex::encode(&hash[..8]); // First 8 bytes = 16 hex chars

        if response != expected_response {
            tracing::debug!(
                "Challenge response mismatch: expected={}, provided={}",
                expected_response,
                response
            );
            return false;
        }

        true
    }

    /// Record the current time as the start of VOTING phase
    /// Used for server-side timing validation of votes
    pub async fn set_voting_phase_started(&self) {
        *self.voting_phase_started_at.write().await = Some(chrono::Utc::now());
        tracing::debug!("Recorded voting phase start time");
    }

    /// Get the timestamp when VOTING phase started (if any)
    pub async fn get_voting_phase_started_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        *self.voting_phase_started_at.read().await
    }

    /// Clear the voting phase start time (e.g., when leaving VOTING phase)
    pub async fn clear_voting_phase_started(&self) {
        *self.voting_phase_started_at.write().await = None;
    }

    // =========================================================================
    // Audience Member Management (auto-generated friendly names)
    // =========================================================================

    /// Helper to capitalize each word
    fn capitalize_name(name: &str) -> String {
        name.split(' ')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Generate a unique friendly name for an audience member
    /// Uses petname crate to generate adjective-noun combinations
    /// Takes a reference to the used_display_names set for O(1) lookups
    fn generate_unique_name(used_names: &HashSet<String>) -> String {
        // Try to generate a unique 2-word name (adjective + noun)
        for _ in 0..100 {
            // petname::petname() generates a name using thread-local RNG
            if let Some(name) = petname::petname(2, " ") {
                let capitalized = Self::capitalize_name(&name);
                if !used_names.contains(&capitalized) {
                    return capitalized;
                }
            }
        }

        // Fallback: add a random number suffix if all attempts fail
        let base_name = petname::petname(2, " ").unwrap_or_else(|| "Mystery Guest".to_string());
        let suffix: u16 = rand::random::<u16>() % 1000;
        format!("{} {}", Self::capitalize_name(&base_name), suffix)
    }

    /// Get or create an audience member with an auto-generated display name
    /// Returns the member (newly created or existing)
    /// Also updates last_seen timestamp for existing members
    pub async fn get_or_create_audience_member(&self, voter_id: &str) -> AudienceMember {
        // Acquire write locks upfront to prevent race conditions.
        // Two concurrent requests for the same voter_id could otherwise both generate names
        // before either inserts, leading to duplicate names or wasted work.
        let mut members = self.audience_members.write().await;
        let mut used_names = self.used_display_names.write().await;

        // Check if member already exists - if so, update last_seen and return
        if let Some(member) = members.get_mut(voter_id) {
            member.last_seen = Some(chrono::Utc::now().to_rfc3339());
            return member.clone();
        }

        // Generate a unique name while holding the lock
        let display_name = Self::generate_unique_name(&used_names);

        // Create and store the new member
        let member = AudienceMember {
            voter_id: voter_id.to_string(),
            display_name: display_name.clone(),
            last_seen: Some(chrono::Utc::now().to_rfc3339()),
        };

        // Insert into both the members map and the used names set
        members.insert(voter_id.to_string(), member.clone());
        used_names.insert(display_name);

        tracing::info!(
            "Created new audience member: {} -> {}",
            voter_id,
            member.display_name
        );

        member
    }

    /// Get an existing audience member by voter_id (returns None if not found)
    pub async fn get_audience_member(&self, voter_id: &str) -> Option<AudienceMember> {
        self.audience_members.read().await.get(voter_id).cloned()
    }

    /// Get all audience members (for export)
    pub async fn get_all_audience_members(&self) -> HashMap<VoterId, AudienceMember> {
        self.audience_members.read().await.clone()
    }

    /// Clear all audience members (for end-of-event cleanup to free memory)
    pub async fn clear_audience_members(&self) {
        let count = self.audience_members.read().await.len();
        self.audience_members.write().await.clear();
        self.used_display_names.write().await.clear();
        tracing::info!("Cleared {} audience members", count);
    }

    /// Cleanup stale audience members who:
    /// - Have 0 points AND
    /// - Haven't connected in the given TTL duration
    ///
    /// Returns the number of members removed
    pub async fn cleanup_stale_audience(&self, ttl_minutes: u64) -> usize {
        use crate::types::ScoreKind;
        use chrono::{DateTime, Duration, Utc};

        let now = Utc::now();
        let ttl = Duration::minutes(ttl_minutes as i64);

        // Get set of voter_ids that have points (should not be removed)
        let voters_with_points: std::collections::HashSet<String> = {
            let scores = self.scores.read().await;
            scores
                .iter()
                .filter(|s| s.kind == ScoreKind::Audience && s.total > 0)
                .map(|s| s.ref_id.clone())
                .collect()
        };

        // Find members to remove
        let members_to_remove: Vec<(String, String)> = {
            let members = self.audience_members.read().await;
            members
                .iter()
                .filter(|(voter_id, member)| {
                    // Skip if they have points
                    if voters_with_points.contains(*voter_id) {
                        return false;
                    }

                    // Check if stale (no last_seen or last_seen > TTL ago)
                    match &member.last_seen {
                        None => true, // No last_seen = stale (legacy data)
                        Some(ts) => {
                            if let Ok(last_seen) = DateTime::parse_from_rfc3339(ts) {
                                now - last_seen.with_timezone(&Utc) > ttl
                            } else {
                                true // Invalid timestamp = stale
                            }
                        }
                    }
                })
                .map(|(voter_id, member)| (voter_id.clone(), member.display_name.clone()))
                .collect()
        };

        let count = members_to_remove.len();
        if count > 0 {
            // Remove from audience_members and used_display_names
            let mut members = self.audience_members.write().await;
            let mut used_names = self.used_display_names.write().await;

            for (voter_id, display_name) in members_to_remove {
                members.remove(&voter_id);
                used_names.remove(&display_name);
            }

            tracing::info!(
                "Cleaned up {} stale audience members (TTL: {} min)",
                count,
                ttl_minutes
            );
        }

        count
    }

    /// Get prompt pool for host (filtered by shadowban status)
    /// Returns prompts sorted by submission_count (popular first), then by created_at (newest first)
    pub async fn get_prompts_for_host(&self) -> Vec<crate::protocol::HostPromptInfo> {
        let pool = self.prompt_pool.read().await;
        let shadowbanned = self.shadowbanned_audience.read().await;

        // Filter out prompts where ALL submitters are shadowbanned
        let mut prompts: Vec<_> = pool
            .values()
            .filter(|p| {
                // Keep prompts with no submitters (host prompts)
                if p.submitter_ids.is_empty() {
                    return true;
                }
                // Keep prompts where at least one submitter is not shadowbanned
                p.submitter_ids.iter().any(|id| !shadowbanned.contains(id))
            })
            .map(|p| crate::protocol::HostPromptInfo {
                id: p.id.clone(),
                text: p.text.clone(),
                image_url: p.image_url.clone(),
                source: p.source.clone(),
                submitter_ids: p.submitter_ids.clone(),
                submission_count: p.submission_count,
                created_at: p.created_at.clone(),
            })
            .collect();

        // Sort by submission_count (descending), then by created_at (newest first)
        prompts.sort_by(|a, b| {
            b.submission_count
                .cmp(&a.submission_count)
                .then_with(|| b.created_at.cmp(&a.created_at))
        });

        prompts
    }

    /// Compute stats about the prompt pool
    pub async fn compute_prompt_pool_stats(&self) -> crate::protocol::PromptPoolStats {
        let pool = self.prompt_pool.read().await;
        let shadowbanned = self.shadowbanned_audience.read().await;

        // Filter out fully shadowbanned prompts for counting
        let visible_prompts: Vec<_> = pool
            .values()
            .filter(|p| {
                if p.submitter_ids.is_empty() {
                    return true;
                }
                p.submitter_ids.iter().any(|id| !shadowbanned.contains(id))
            })
            .collect();

        let total = visible_prompts.len();
        let host_count = visible_prompts
            .iter()
            .filter(|p| p.source == PromptSource::Host)
            .count();
        let audience_count = total - host_count;

        // Count submissions per voter (across all prompts)
        let mut submitter_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for prompt in &visible_prompts {
            for submitter_id in &prompt.submitter_ids {
                if !shadowbanned.contains(submitter_id) {
                    *submitter_counts.entry(submitter_id.as_str()).or_insert(0) += 1;
                }
            }
        }

        // Get top 5 submitters
        let mut top_submitters: Vec<_> = submitter_counts
            .into_iter()
            .map(|(voter_id, count)| crate::protocol::SubmitterStats {
                voter_id: voter_id.to_string(),
                count,
            })
            .collect();
        top_submitters.sort_by(|a, b| b.count.cmp(&a.count));
        top_submitters.truncate(5);

        crate::protocol::PromptPoolStats {
            total,
            host_count,
            audience_count,
            top_submitters,
        }
    }

    /// Broadcast prompt pool to host (filtered by shadowban status)
    /// Used for immediate broadcasts (host actions)
    pub async fn broadcast_prompts_to_host(&self) {
        let prompts = self.get_prompts_for_host().await;
        let stats = self.compute_prompt_pool_stats().await;
        self.broadcast_to_host(ServerMessage::HostPrompts { prompts, stats });
    }

    /// Mark prompts as dirty for debounced broadcast (audience submissions)
    pub fn mark_prompts_dirty(&self) {
        self.prompts_dirty.store(true, Ordering::SeqCst);
    }

    /// Broadcast prompt pool to host if dirty flag is set
    /// Called periodically by background task for debounced updates
    pub async fn broadcast_prompts_to_host_if_dirty(&self) {
        if self.prompts_dirty.swap(false, Ordering::SeqCst) {
            let prompts = self.get_prompts_for_host().await;
            let stats = self.compute_prompt_pool_stats().await;
            self.broadcast_to_host(ServerMessage::HostPrompts { prompts, stats });
        }
    }

    /// Maximum number of prompts an audience member can submit before their prompts are used
    const MAX_PROMPTS_PER_USER: usize = 10;

    /// Maximum length of prompt text (chars) to prevent memory abuse
    const MAX_PROMPT_TEXT_LENGTH: usize = 500;

    /// Maximum size of the global prompt pool to prevent unbounded growth
    const MAX_PROMPT_POOL_SIZE: usize = 1000;

    /// Fuzzy similarity threshold for deduplication (0.0 - 1.0)
    const SIMILARITY_THRESHOLD: f64 = 0.6;

    /// Compute Jaccard similarity between two texts (word-based)
    fn compute_similarity(text1: &str, text2: &str) -> f64 {
        // Normalize: lowercase, remove punctuation, split into words
        fn normalize_to_words(s: &str) -> std::collections::HashSet<String> {
            s.to_lowercase()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c.is_whitespace() {
                        c
                    } else {
                        ' '
                    }
                })
                .collect::<String>()
                .split_whitespace()
                .filter(|w| w.len() > 2) // Skip very short words
                .map(|s| s.to_string())
                .collect()
        }

        let words1 = normalize_to_words(text1);
        let words2 = normalize_to_words(text2);

        if words1.is_empty() && words2.is_empty() {
            return 1.0; // Both empty = identical
        }
        if words1.is_empty() || words2.is_empty() {
            return 0.0; // One empty = not similar
        }

        let intersection: std::collections::HashSet<_> = words1.intersection(&words2).collect();
        let union: std::collections::HashSet<_> = words1.union(&words2).collect();

        intersection.len() as f64 / union.len() as f64
    }

    /// Find a similar prompt in the pool (for deduplication)
    /// Returns the prompt ID if a similar prompt is found
    pub async fn find_similar_prompt(&self, text: &str) -> Option<PromptId> {
        let pool = self.prompt_pool.read().await;

        for prompt in pool.values() {
            if let Some(ref existing_text) = prompt.text {
                let similarity = Self::compute_similarity(text, existing_text);
                if similarity >= Self::SIMILARITY_THRESHOLD {
                    return Some(prompt.id.clone());
                }
            }
        }

        None
    }

    /// Count how many prompts a user has in the pool (for throttling)
    pub async fn count_user_prompts(&self, voter_id: &str) -> usize {
        self.prompt_pool
            .read()
            .await
            .values()
            .filter(|p| p.submitter_ids.contains(&voter_id.to_string()))
            .count()
    }

    /// Add a prompt to the global pool
    /// Handles deduplication: if similar prompt exists, adds submitter to existing prompt
    /// Handles throttling: rejects if user has >= 10 pending prompts
    /// Handles size limits: truncates long text, rejects if pool is full (for audience)
    pub async fn add_prompt_to_pool(
        &self,
        text: Option<String>,
        image_url: Option<String>,
        source: PromptSource,
        submitter_id: Option<VoterId>,
    ) -> Result<Prompt, String> {
        // Validate: must have either text or image_url
        if text.is_none() && image_url.is_none() {
            return Err("Prompt must have either text or image_url".to_string());
        }

        // Validate image URL if provided - block dangerous schemes but allow relative URLs
        if let Some(ref url) = image_url {
            // Only validate absolute URLs (relative URLs fail to parse and are fine)
            if let Ok(parsed) = reqwest::Url::parse(url) {
                if !["http", "https"].contains(&parsed.scheme()) {
                    return Err("Image URL must use http or https".to_string());
                }
            }
        }

        // Truncate text if too long (prevents memory abuse)
        let text = text.map(|t| {
            if t.chars().count() > Self::MAX_PROMPT_TEXT_LENGTH {
                t.chars().take(Self::MAX_PROMPT_TEXT_LENGTH).collect()
            } else {
                t
            }
        });

        // Pool size limit (only enforced for audience submissions, not host)
        if source == PromptSource::Audience {
            let pool_size = self.prompt_pool.read().await.len();
            if pool_size >= Self::MAX_PROMPT_POOL_SIZE {
                return Err(
                    "POOL_FULL: Der Prompt-Pool ist voll. Bitte warte, bis Prompts verwendet wurden."
                        .to_string(),
                );
            }
        }

        // Throttling: check if audience member has too many prompts
        if let Some(ref voter_id) = submitter_id {
            let count = self.count_user_prompts(voter_id).await;
            if count >= Self::MAX_PROMPTS_PER_USER {
                return Err(format!(
                    "PROMPT_LIMIT_REACHED: Du hast bereits {} Prompts eingereicht. Warte, bis deine Prompts verwendet wurden.",
                    Self::MAX_PROMPTS_PER_USER
                ));
            }
        }

        // Fuzzy deduplication: check for similar prompts (only for text prompts, skip for host)
        if source != PromptSource::Host {
            if let Some(ref prompt_text) = text {
                if let Some(similar_id) = self.find_similar_prompt(prompt_text).await {
                    // Found similar prompt - add submitter to existing prompt instead
                    let mut pool = self.prompt_pool.write().await;
                    if let Some(existing) = pool.get_mut(&similar_id) {
                        // Add submitter if not already in list
                        if let Some(ref voter_id) = submitter_id {
                            if !existing.submitter_ids.contains(voter_id) {
                                existing.submitter_ids.push(voter_id.clone());
                            }
                        }
                        existing.submission_count += 1;
                        return Ok(existing.clone());
                    }
                }
            }
        }

        // No similar prompt found - create new one
        let now = chrono::Utc::now().to_rfc3339();
        let submitter_ids = submitter_id.map(|id| vec![id]).unwrap_or_default();

        let prompt = Prompt {
            id: ulid::Ulid::new().to_string(),
            text,
            image_url,
            source,
            submitter_ids,
            submission_count: 1,
            created_at: Some(now),
        };

        self.prompt_pool
            .write()
            .await
            .insert(prompt.id.clone(), prompt.clone());
        Ok(prompt)
    }

    /// Shadowban all submitters of a prompt (bulk shadowban for spam)
    pub async fn shadowban_prompt_submitters(
        &self,
        prompt_id: &str,
    ) -> Result<Vec<VoterId>, String> {
        let pool = self.prompt_pool.read().await;
        let prompt = pool
            .get(prompt_id)
            .ok_or_else(|| "Prompt not found".to_string())?;

        let submitter_ids = prompt.submitter_ids.clone();
        drop(pool);

        // Shadowban all submitters
        for voter_id in &submitter_ids {
            self.shadowban_audience(voter_id.clone()).await;
        }

        // Remove the prompt from the pool
        self.remove_prompt_from_pool(prompt_id).await;

        Ok(submitter_ids)
    }

    /// Remove a prompt from the pool by ID (used when selecting or deleting)
    pub async fn remove_prompt_from_pool(&self, prompt_id: &str) -> Option<Prompt> {
        self.prompt_pool.write().await.remove(prompt_id)
    }

    /// Get a prompt from the pool by ID (without removing)
    pub async fn get_prompt_from_pool(&self, prompt_id: &str) -> Option<Prompt> {
        self.prompt_pool.read().await.get(prompt_id).cloned()
    }

    // =========================================================================
    // Queued Prompts Management (for PROMPT_SELECTION phase)
    // =========================================================================

    /// Queue a prompt for the next round (move from pool to queue)
    /// Max 3 prompts can be queued at a time
    pub async fn queue_prompt(&self, prompt_id: &str) -> Result<Prompt, String> {
        // Check if already at max
        let queue_len = self.queued_prompts.read().await.len();
        if queue_len >= 3 {
            return Err("Maximum 3 prompts can be queued".to_string());
        }

        // Check if already queued
        if self
            .queued_prompts
            .read()
            .await
            .iter()
            .any(|p| p.id == prompt_id)
        {
            return Err("Prompt already queued".to_string());
        }

        // Remove from pool and add to queue
        let prompt = self
            .remove_prompt_from_pool(prompt_id)
            .await
            .ok_or_else(|| "Prompt not found in pool".to_string())?;

        self.queued_prompts.write().await.push(prompt.clone());
        Ok(prompt)
    }

    /// Unqueue a prompt (move from queue back to pool)
    pub async fn unqueue_prompt(&self, prompt_id: &str) -> Result<Prompt, String> {
        let mut queue = self.queued_prompts.write().await;
        let pos = queue
            .iter()
            .position(|p| p.id == prompt_id)
            .ok_or_else(|| "Prompt not found in queue".to_string())?;

        let prompt = queue.remove(pos);
        drop(queue);

        // Add back to pool
        self.prompt_pool
            .write()
            .await
            .insert(prompt.id.clone(), prompt.clone());
        Ok(prompt)
    }

    /// Delete a prompt permanently from either pool or queue
    /// Returns true if the prompt was found and deleted
    pub async fn delete_prompt(&self, prompt_id: &str) -> bool {
        // Try to remove from queue first
        {
            let mut queue = self.queued_prompts.write().await;
            if let Some(pos) = queue.iter().position(|p| p.id == prompt_id) {
                queue.remove(pos);
                return true;
            }
        }

        // Try to remove from pool
        self.prompt_pool.write().await.remove(prompt_id).is_some()
    }

    /// Get all queued prompts
    pub async fn get_queued_prompts(&self) -> Vec<Prompt> {
        self.queued_prompts.read().await.clone()
    }

    /// Clear all queued prompts (move back to pool)
    pub async fn clear_queued_prompts(&self) {
        let mut queue = self.queued_prompts.write().await;
        let prompts: Vec<Prompt> = queue.drain(..).collect();
        drop(queue);

        // Move all back to pool
        let mut pool = self.prompt_pool.write().await;
        for prompt in prompts {
            pool.insert(prompt.id.clone(), prompt);
        }
    }

    /// Select winning prompt from queue based on votes (or the only one if single)
    /// Returns the winning prompt and removes it from queue
    /// Remaining prompts are moved back to pool
    pub async fn select_winning_prompt(&self) -> Result<Prompt, String> {
        let queue = self.queued_prompts.read().await;
        if queue.is_empty() {
            return Err("No prompts queued".to_string());
        }

        let winner = if queue.len() == 1 {
            // Only one prompt, it wins automatically
            queue[0].clone()
        } else {
            // Multiple prompts, count votes
            let votes = self.prompt_votes.read().await;
            let mut counts: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();

            for prompt_id in votes.values() {
                // Only count votes for queued prompts
                if queue.iter().any(|p| &p.id == prompt_id) {
                    *counts.entry(prompt_id.as_str()).or_insert(0) += 1;
                }
            }

            // Find winner (highest votes, or first prompt if tie/no votes)
            let winner_id = queue
                .iter()
                .max_by_key(|p| counts.get(p.id.as_str()).unwrap_or(&0))
                .map(|p| p.id.clone())
                .unwrap();

            queue
                .iter()
                .find(|p| p.id == winner_id)
                .cloned()
                .ok_or_else(|| "Winner not found".to_string())?
        };
        drop(queue);

        // Remove winner from queue, return losers to pool
        let mut queue = self.queued_prompts.write().await;
        queue.retain(|p| p.id != winner.id);

        // Move remaining (non-winning) prompts back to pool for reuse
        let losers: Vec<Prompt> = queue.drain(..).collect();
        drop(queue);

        if !losers.is_empty() {
            let mut pool = self.prompt_pool.write().await;
            for loser in losers {
                pool.insert(loser.id.clone(), loser);
            }
        }

        // Clear prompt votes for next round
        self.prompt_votes.write().await.clear();

        Ok(winner)
    }

    /// Record a prompt vote from an audience member
    pub async fn record_prompt_vote(&self, voter_id: &str, prompt_id: &str) -> Result<(), String> {
        // Verify prompt is in queue
        let queue = self.queued_prompts.read().await;
        if !queue.iter().any(|p| p.id == prompt_id) {
            return Err("Prompt not in queue".to_string());
        }
        drop(queue);

        // Record vote (overwrites previous vote from same voter)
        self.prompt_votes
            .write()
            .await
            .insert(voter_id.to_string(), prompt_id.to_string());

        Ok(())
    }

    /// Get prompt vote counts for display
    pub async fn get_prompt_vote_counts(&self) -> std::collections::HashMap<String, u32> {
        let votes = self.prompt_votes.read().await;
        let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

        for prompt_id in votes.values() {
            *counts.entry(prompt_id.clone()).or_insert(0) += 1;
        }

        counts
    }

    /// Broadcast queued prompts to host
    pub async fn broadcast_queued_prompts_to_host(&self) {
        let prompts = self.get_queued_prompts().await;
        let prompt_infos: Vec<crate::protocol::HostPromptInfo> = prompts
            .iter()
            .map(|p| crate::protocol::HostPromptInfo {
                id: p.id.clone(),
                text: p.text.clone(),
                image_url: p.image_url.clone(),
                source: p.source.clone(),
                submitter_ids: p.submitter_ids.clone(),
                submission_count: p.submission_count,
                created_at: p.created_at.clone(),
            })
            .collect();
        self.broadcast_to_host(ServerMessage::HostQueuedPrompts {
            prompts: prompt_infos,
        });
    }

    /// Export the entire game state as a serializable snapshot.
    ///
    /// Acquires all locks to ensure a consistent snapshot.
    pub async fn export_state(&self) -> GameStateExport {
        // Acquire all locks to get a consistent snapshot
        let game = self.game.read().await.clone();
        let rounds = self.rounds.read().await.clone();
        let submissions = self.submissions.read().await.clone();
        let votes = self.votes.read().await.clone();
        let players = self.players.read().await.clone();
        let scores = self.scores.read().await.clone();
        let processed_vote_msg_ids = self.processed_vote_msg_ids.read().await.clone();
        let player_status = self.player_status.read().await.clone();
        let shadowbanned_audience = self.shadowbanned_audience.read().await.clone();
        // Convert HashMap to Vec for backwards-compatible export format
        let prompt_pool: Vec<Prompt> = self.prompt_pool.read().await.values().cloned().collect();
        let audience_members = self.audience_members.read().await.clone();
        let trivia_questions = self.trivia_questions.read().await.clone();

        GameStateExport::new(
            game,
            rounds,
            submissions,
            votes,
            players,
            scores,
            processed_vote_msg_ids,
            player_status,
            shadowbanned_audience,
            prompt_pool,
            audience_members,
            trivia_questions,
        )
    }

    /// Import a state snapshot, replacing all current state.
    ///
    /// This validates the import first, then atomically replaces all state.
    /// After import, broadcasts a full state refresh to all connected clients.
    pub async fn import_state(&self, export: GameStateExport) -> Result<(), String> {
        // Validate before importing
        export.validate()?;

        // Acquire all write locks and replace state
        *self.game.write().await = export.game.clone();
        *self.rounds.write().await = export.rounds;

        // Rebuild submissions and secondary index
        {
            let mut submissions = self.submissions.write().await;
            let mut by_round = self.submissions_by_round.write().await;
            *submissions = export.submissions;
            // Rebuild index from submissions
            by_round.clear();
            for (id, sub) in submissions.iter() {
                by_round
                    .entry(sub.round_id.clone())
                    .or_default()
                    .insert(id.clone());
            }
        }

        *self.votes.write().await = export.votes;
        *self.players.write().await = export.players;
        *self.scores.write().await = export.scores;
        *self.processed_vote_msg_ids.write().await = export.processed_vote_msg_ids;
        *self.player_status.write().await = export.player_status;
        *self.shadowbanned_audience.write().await = export.shadowbanned_audience;
        // Convert Vec to HashMap for internal storage
        *self.prompt_pool.write().await = export
            .prompt_pool
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();
        *self.audience_members.write().await = export.audience_members.clone();
        *self.trivia_questions.write().await = export.trivia_questions;

        // Rebuild used_display_names from imported audience_members
        *self.used_display_names.write().await = export
            .audience_members
            .values()
            .map(|m| m.display_name.clone())
            .collect();

        // Broadcast state refresh to all clients
        if let Some(ref game) = export.game {
            let valid_transitions = Self::get_valid_transitions(&game.phase);
            self.broadcast_to_all(ServerMessage::GameState {
                game: game.clone(),
                valid_transitions,
            });
        }

        tracing::info!("State imported successfully");
        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_game() {
        let state = AppState::new();
        let game = state.create_game().await;

        assert_eq!(game.phase, GamePhase::Lobby);
        assert_eq!(game.round_no, 0);
        assert!(state.get_game().await.is_some());
    }

    #[tokio::test]
    async fn test_create_player() {
        let state = AppState::new();
        let player = state.create_player().await;

        assert!(player.display_name.is_none());
        assert!(!player.token.is_empty());
        assert!(state.get_player_by_token(&player.token).await.is_some());
    }

    #[tokio::test]
    async fn test_register_player() {
        let state = AppState::new();
        let player = state.create_player().await;
        let token = player.token.clone();

        let result = state
            .register_player(&token, "TestPlayer".to_string())
            .await;
        assert!(result.is_ok());

        let registered = result.unwrap();
        assert_eq!(registered.display_name, Some("TestPlayer".to_string()));
    }

    #[tokio::test]
    async fn test_round_lifecycle() {
        let state = AppState::new();
        state.create_game().await;

        let round = state.start_round().await.unwrap();
        assert_eq!(round.number, 1);
        assert_eq!(round.state, RoundState::Setup);

        let current = state.get_current_round().await;
        assert!(current.is_some());
        assert_eq!(current.unwrap().id, round.id);
    }

    // GamePhase validation tests

    #[tokio::test]
    async fn test_valid_game_phase_transitions() {
        let state = AppState::new();
        state.create_game().await;

        // Queue 2 prompts so we can test PromptSelection
        // Use distinct prompts that won't be merged by fuzzy deduplication
        let prompt1 = state
            .add_prompt_to_pool(
                Some("Apple fruit basket".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        let prompt2 = state
            .add_prompt_to_pool(
                Some("Banana orange juice".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state.queue_prompt(&prompt1.id).await.unwrap();
        state.queue_prompt(&prompt2.id).await.unwrap();

        // Lobby -> PromptSelection (with 2 prompts, stays in PromptSelection)
        assert!(state
            .transition_phase(GamePhase::PromptSelection, None)
            .await
            .is_ok());

        // PromptSelection -> Writing (auto-selects winning prompt)
        // Note: This is now valid because it auto-selects winning prompt
        // Skip for now, test separately

        // Test panic mode: any phase -> Intermission
        assert!(state
            .transition_phase(GamePhase::Intermission, None)
            .await
            .is_ok());

        // Intermission -> any phase
        assert!(state.transition_phase(GamePhase::Lobby, None).await.is_ok());

        // Test hard stop: any phase -> Ended
        assert!(state.transition_phase(GamePhase::Ended, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_game_phase_transitions() {
        let state = AppState::new();
        state.create_game().await;

        // Lobby -> Writing is now valid (with preconditions), but fails without round/prompt
        let result = state.transition_phase(GamePhase::Writing, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Writing phase requires"));

        // Can't go from Lobby to Voting (invalid transition)
        let result = state.transition_phase(GamePhase::Voting, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid phase transition"));

        // Can't go from Lobby to Reveal (invalid transition)
        let result = state.transition_phase(GamePhase::Reveal, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid phase transition"));
    }

    #[tokio::test]
    async fn test_game_phase_preconditions() {
        let state = AppState::new();
        state.create_game().await;

        // Queue 2 prompts so we stay in PromptSelection (single prompt auto-advances)
        // Use distinct prompts that won't be merged by fuzzy deduplication
        let prompt1 = state
            .add_prompt_to_pool(
                Some("Mountain hiking adventure".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        let prompt2 = state
            .add_prompt_to_pool(
                Some("Ocean swimming beach".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state.queue_prompt(&prompt1.id).await.unwrap();
        state.queue_prompt(&prompt2.id).await.unwrap();

        // Transition to PromptSelection
        state
            .transition_phase(GamePhase::PromptSelection, None)
            .await
            .unwrap();

        // With new flow, going to Writing from PromptSelection auto-selects winner and creates round
        // So the test needs to verify something different - let's verify PromptSelection requires queued prompts
        let state2 = AppState::new();
        state2.create_game().await;
        let result = state2
            .transition_phase(GamePhase::PromptSelection, None)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Prompt selection requires at least 1 queued prompt"));
    }

    #[tokio::test]
    async fn test_writing_phase_requires_prompt() {
        let state = AppState::new();
        state.create_game().await;

        // Test direct Lobby -> Writing requires round with prompt
        // First, trying without a round should fail
        let result = state.transition_phase(GamePhase::Writing, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Writing phase requires"));

        // Create a round but don't select a prompt
        let round = state.start_round().await.unwrap();
        let result = state.transition_phase(GamePhase::Writing, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("selected prompt"));

        // Add prompt to pool and select it
        let prompt = state
            .add_prompt_to_pool(
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();

        // Now transition should work
        assert!(state
            .transition_phase(GamePhase::Writing, None)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_reveal_phase_requires_submissions() {
        let state = AppState::new();
        state.create_game().await;

        // Create round and select prompt (direct Lobby->Writing path)
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();

        // Try to go to Reveal without submissions
        let result = state.transition_phase(GamePhase::Reveal, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one submission"));
    }

    #[tokio::test]
    async fn test_reveal_auto_populates_reveal_order() {
        let state = AppState::new();
        state.create_game().await;

        // Create round and select prompt (direct Lobby->Writing path)
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();

        // Add a submission
        let player = state.create_player().await;
        let sub = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Test answer".to_string(),
            )
            .await
            .unwrap();

        // Transition to Reveal should auto-populate reveal_order
        state
            .transition_phase(GamePhase::Reveal, None)
            .await
            .unwrap();

        // Check reveal_order was auto-populated
        let current_round = state.get_current_round().await.unwrap();
        assert!(!current_round.reveal_order.is_empty());
        assert!(current_round.reveal_order.contains(&sub.id));
    }

    // RoundState validation tests

    #[tokio::test]
    async fn test_valid_round_state_transitions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add prompt to pool and select it
        let prompt = state
            .add_prompt_to_pool(
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();

        // Should now be in Collecting state
        let current = state.get_current_round().await.unwrap();
        assert_eq!(current.state, RoundState::Collecting);
    }

    #[tokio::test]
    async fn test_invalid_round_state_transitions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Can't go from Setup to Revealing
        let result = state
            .transition_round_state(&round.id, RoundState::Revealing)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid round state transition"));
    }

    #[tokio::test]
    async fn test_round_state_preconditions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Can't transition to Collecting without selected prompt
        let result = state
            .transition_round_state(&round.id, RoundState::Collecting)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("selected prompt"));
    }

    #[tokio::test]
    async fn test_revealing_requires_submissions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        let prompt = state
            .add_prompt_to_pool(
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();

        // Try to transition to Revealing without submissions
        let result = state
            .transition_round_state(&round.id, RoundState::Revealing)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one submission"));
    }

    #[tokio::test]
    async fn test_select_prompt_validates_state() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add both prompts to pool first
        // Use distinct prompts that won't be merged by fuzzy deduplication
        let prompt = state
            .add_prompt_to_pool(
                Some("First unique question".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        let prompt2 = state
            .add_prompt_to_pool(
                Some("Second different topic".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();

        // First selection should work (transitions round to Collecting)
        assert!(state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .is_ok());

        // Try to select second prompt when not in Setup state
        let result = state.select_prompt(&round.id, &prompt2.id, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Setup state"));
    }

    #[tokio::test]
    async fn test_start_round_validates_phase() {
        let state = AppState::new();
        state.create_game().await;

        // First round should work in Lobby
        let round = state.start_round().await.unwrap();

        // Add prompt and select it, then transition to Writing phase
        let prompt = state
            .add_prompt_to_pool(Some("Test".to_string()), None, PromptSource::Host, None)
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();

        // Should not be able to start round in Writing phase
        let result = state.start_round().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot start round"));
    }

    #[tokio::test]
    async fn test_start_round_requires_closed_previous_round() {
        let state = AppState::new();
        state.create_game().await;

        let _round = state.start_round().await.unwrap();

        // Try to start another round while first is still in Setup
        let result = state.start_round().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("current round is in"));
    }

    #[tokio::test]
    async fn test_set_reveal_order_validates_submissions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Empty order should fail
        let result = state.set_reveal_order(&round.id, vec![]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be empty"));

        // Non-existent submission should fail
        let result = state
            .set_reveal_order(&round.id, vec!["fake_id".to_string()])
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));

        // Create a submission
        let player = state.create_player().await;
        let submission = state
            .submit_answer(&round.id, Some(player.id.clone()), "Test".to_string())
            .await
            .unwrap();

        // Valid order should work
        let result = state
            .set_reveal_order(&round.id, vec![submission.id.clone()])
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_reveal_order_validates_round_ownership() {
        let state = AppState::new();
        state.create_game().await;

        // Create first round
        let round1 = state.start_round().await.unwrap();

        // Close first round by transitioning through phases
        let mut rounds = state.rounds.write().await;
        if let Some(r) = rounds.get_mut(&round1.id) {
            r.state = RoundState::Closed;
        }
        drop(rounds);

        // Create second round (directly from Lobby, skipping PromptSelection)
        let round2 = state.start_round().await.unwrap();

        // Create submission in round 2
        let player = state.create_player().await;
        let submission = state
            .submit_answer(&round2.id, Some(player.id), "Test".to_string())
            .await
            .unwrap();

        // Try to use round2's submission in round1's reveal order
        let result = state
            .set_reveal_order(&round1.id, vec![submission.id])
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not belong to round"));
    }

    #[tokio::test]
    async fn test_results_phase_requires_ai_submission() {
        let state = AppState::new();
        state.create_game().await;

        // Create round and select prompt (direct path)
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();

        // Add submissions
        let player = state.create_player().await;
        let sub1 = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Player answer".to_string(),
            )
            .await
            .unwrap();
        let _sub2 = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();

        // Set reveal order
        state
            .set_reveal_order(&round.id, vec![sub1.id.clone()])
            .await
            .unwrap();

        // Progress through phases
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Reveal, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Voting, None)
            .await
            .unwrap();

        // Try to go to RESULTS without setting AI submission
        let result = state.transition_phase(GamePhase::Results, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AI submission to be set"));
    }

    #[tokio::test]
    async fn test_scoring_is_idempotent() {
        let state = AppState::new();
        state.create_game().await;

        // Create round and select prompt directly
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(Some("Test".to_string()), None, PromptSource::Host, None)
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();

        let player = state.create_player().await;
        let player_sub = state
            .submit_answer(&round.id, Some(player.id.clone()), "Player".to_string())
            .await
            .unwrap();
        let ai_sub = state
            .submit_answer(&round.id, None, "AI".to_string())
            .await
            .unwrap();

        state
            .set_ai_submission(&round.id, ai_sub.id.clone())
            .await
            .unwrap();
        state
            .set_reveal_order(&round.id, vec![player_sub.id.clone(), ai_sub.id.clone()])
            .await
            .unwrap();

        // Add a vote
        let vote = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round.id.clone(),
            voter_id: "voter1".to_string(),
            ai_pick_submission_id: player_sub.id.clone(),
            funny_pick_submission_id: player_sub.id.clone(),
            ts: chrono::Utc::now().to_rfc3339(),
        };
        state.votes.write().await.insert(vote.id.clone(), vote);

        // Progress to RESULTS (first time)
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Reveal, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Voting, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Results, None)
            .await
            .unwrap();

        let (scores1, _) = state.get_leaderboards().await;
        assert_eq!(scores1.len(), 1);
        assert_eq!(scores1[0].total, 2); // 1 AI + 1 funny

        // Re-enter RESULTS (should not duplicate scores)
        state
            .transition_phase(GamePhase::Intermission, None)
            .await
            .unwrap();
        state
            .transition_phase(GamePhase::Results, None)
            .await
            .unwrap();

        let (scores2, _) = state.get_leaderboards().await;
        assert_eq!(scores2.len(), 1);
        assert_eq!(scores2[0].total, 2); // Still 2, not 4!
    }

    #[tokio::test]
    async fn test_exact_duplicate_detection() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // First submission succeeds
        let result = state
            .submit_answer(&round.id, None, "Test answer".to_string())
            .await;
        assert!(result.is_ok());

        // Exact duplicate fails
        let result = state
            .submit_answer(&round.id, None, "Test answer".to_string())
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "DUPLICATE_EXACT");
    }

    #[tokio::test]
    async fn test_duplicate_detection_case_insensitive() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // First submission succeeds
        state
            .submit_answer(&round.id, None, "Test Answer".to_string())
            .await
            .unwrap();

        // Same text different case fails
        let result = state
            .submit_answer(&round.id, None, "test answer".to_string())
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "DUPLICATE_EXACT");

        // Different case with whitespace also fails
        let result = state
            .submit_answer(&round.id, None, "  TEST ANSWER  ".to_string())
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "DUPLICATE_EXACT");
    }

    #[tokio::test]
    async fn test_mark_submission_duplicate() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create a player and their submission
        let player = state.create_player().await;
        let sub = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Player answer".to_string(),
            )
            .await
            .unwrap();

        // Verify submission exists
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 1);

        // Mark as duplicate
        let result = state.mark_submission_duplicate(&sub.id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(player.id));

        // Verify submission is removed
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 0);
    }

    #[tokio::test]
    async fn test_mark_ai_submission_duplicate() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create AI submission
        let sub = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();

        // Mark as duplicate - returns None for AI submissions
        let result = state.mark_submission_duplicate(&sub.id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Verify submission is removed
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 0);
    }

    #[tokio::test]
    async fn test_mark_nonexistent_duplicate() {
        let state = AppState::new();
        state.create_game().await;

        let result = state.mark_submission_duplicate("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // Shadowban tests

    #[tokio::test]
    async fn test_shadowban_audience() {
        let state = AppState::new();

        // Initially not shadowbanned
        assert!(!state.is_shadowbanned("voter1").await);

        // Shadowban the voter
        state.shadowban_audience("voter1".to_string()).await;

        // Now should be shadowbanned
        assert!(state.is_shadowbanned("voter1").await);

        // Other voters unaffected
        assert!(!state.is_shadowbanned("voter2").await);
    }

    #[tokio::test]
    async fn test_shadowban_filters_prompts() {
        let state = AppState::new();
        state.create_game().await;

        // Add prompts from different audience members to the global pool
        state
            .add_prompt_to_pool(
                Some("Prompt from voter1".to_string()),
                None,
                PromptSource::Audience,
                Some("voter1".to_string()),
            )
            .await
            .unwrap();
        state
            .add_prompt_to_pool(
                Some("Prompt from voter2".to_string()),
                None,
                PromptSource::Audience,
                Some("voter2".to_string()),
            )
            .await
            .unwrap();
        state
            .add_prompt_to_pool(
                Some("Host prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();

        // Before shadowban: all 3 prompts should be visible in pool
        let pool = state.prompt_pool.read().await;
        assert_eq!(pool.len(), 3);
        drop(pool);

        // Shadowban voter1
        state.shadowban_audience("voter1".to_string()).await;

        // The prompts are still stored in pool, but get_prompts_for_host filters them
        // Let's verify the shadowban set contains voter1
        assert!(state.is_shadowbanned("voter1").await);

        // Get shadowbanned list
        let shadowbanned = state.get_shadowbanned_audience().await;
        assert_eq!(shadowbanned.len(), 1);
        assert!(shadowbanned.contains(&"voter1".to_string()));

        // Verify get_prompts_for_host filters out shadowbanned voter's prompts
        let filtered_prompts = state.get_prompts_for_host().await;
        assert_eq!(filtered_prompts.len(), 2); // voter2's prompt + host prompt
    }

    #[tokio::test]
    async fn test_prompt_submitter_id_tracked() {
        let state = AppState::new();
        state.create_game().await;

        // Add a prompt with submitter_id to the global pool
        let prompt = state
            .add_prompt_to_pool(
                Some("Test prompt".to_string()),
                None,
                PromptSource::Audience,
                Some("voter123".to_string()),
            )
            .await
            .unwrap();

        // Verify submitter_id was stored
        assert!(prompt.submitter_ids.contains(&"voter123".to_string()));

        // Verify it's in the prompt pool
        let pool = state.prompt_pool.read().await;
        let stored_prompt = pool.get(&prompt.id).unwrap();
        assert!(stored_prompt
            .submitter_ids
            .contains(&"voter123".to_string()));
    }

    // Remove player tests

    #[tokio::test]
    async fn test_remove_player_basic() {
        let state = AppState::new();
        state.create_game().await;

        // Create a player
        let player = state.create_player().await;
        let player_id = player.id.clone();

        // Verify player exists
        assert!(state.get_player_by_token(&player.token).await.is_some());
        assert_eq!(state.players.read().await.len(), 1);

        // Remove the player
        let result = state.remove_player(&player_id).await;
        assert!(result.is_ok());

        // Verify player is gone
        assert!(state.get_player_by_token(&player.token).await.is_none());
        assert_eq!(state.players.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_player_not_found() {
        let state = AppState::new();
        state.create_game().await;

        let result = state.remove_player(&"nonexistent".to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_remove_player_removes_submission() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create player and submit answer
        let player = state.create_player().await;
        let player_id = player.id.clone();
        state
            .submit_answer(
                &round.id,
                Some(player_id.clone()),
                "Test answer".to_string(),
            )
            .await
            .unwrap();

        // Verify submission exists
        assert_eq!(state.get_submissions(&round.id).await.len(), 1);

        // Remove player
        state.remove_player(&player_id).await.unwrap();

        // Verify submission is removed
        assert_eq!(state.get_submissions(&round.id).await.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_player_updates_reveal_order() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create two players with submissions
        let player1 = state.create_player().await;
        let player2 = state.create_player().await;

        let sub1 = state
            .submit_answer(&round.id, Some(player1.id.clone()), "Answer 1".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, Some(player2.id.clone()), "Answer 2".to_string())
            .await
            .unwrap();

        // Set reveal order
        state
            .set_reveal_order(&round.id, vec![sub1.id.clone(), sub2.id.clone()])
            .await
            .unwrap();

        // Verify reveal order has both
        let round_data = state.get_current_round().await.unwrap();
        assert_eq!(round_data.reveal_order.len(), 2);

        // Remove player1
        state.remove_player(&player1.id).await.unwrap();

        // Verify reveal order only has player2's submission
        let round_data = state.get_current_round().await.unwrap();
        assert_eq!(round_data.reveal_order.len(), 1);
        assert_eq!(round_data.reveal_order[0], sub2.id);
    }

    #[tokio::test]
    async fn test_remove_player_resets_affected_votes() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create audience members (required for voter validation)
        state.get_or_create_audience_member("voter1").await;
        state.get_or_create_audience_member("voter2").await;

        // Create player with submission
        let player = state.create_player().await;
        let sub = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Player answer".to_string(),
            )
            .await
            .unwrap();

        // Create another submission (AI) to vote for funny
        let ai_sub = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();

        // Set game to VOTING phase (votes only accepted during voting)
        state.game.write().await.as_mut().unwrap().phase = GamePhase::Voting;

        // Add a vote that references the player's submission
        state
            .submit_vote(
                "voter1".to_string(),
                sub.id.clone(), // AI pick points to player's submission
                ai_sub.id.clone(),
                "msg1".to_string(),
            )
            .await;

        // Another vote that doesn't reference the player's submission
        state
            .submit_vote(
                "voter2".to_string(),
                ai_sub.id.clone(),
                ai_sub.id.clone(),
                "msg2".to_string(),
            )
            .await;

        // Verify we have 2 votes
        assert_eq!(state.votes.read().await.len(), 2);

        // Remove player
        state.remove_player(&player.id).await.unwrap();

        // voter1's vote should be removed (referenced player's submission)
        // voter2's vote should remain (only referenced AI submission)
        assert_eq!(state.votes.read().await.len(), 1);

        // voter1 should be able to vote again (msg_id cleared)
        let processed = state.processed_vote_msg_ids.read().await;
        assert!(!processed.contains_key("voter1"));
        assert!(processed.contains_key("voter2"));
    }

    #[tokio::test]
    async fn test_remove_player_adjusts_reveal_index() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create two players with submissions
        let player1 = state.create_player().await;
        let player2 = state.create_player().await;

        let sub1 = state
            .submit_answer(&round.id, Some(player1.id.clone()), "Answer 1".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, Some(player2.id.clone()), "Answer 2".to_string())
            .await
            .unwrap();

        // Set reveal order and advance reveal index to end
        state
            .set_reveal_order(&round.id, vec![sub1.id.clone(), sub2.id.clone()])
            .await
            .unwrap();

        // Advance reveal to the second submission (index 1)
        state.reveal_next(&round.id).await.unwrap();

        // Verify reveal_index is 1
        let round_data = state.get_current_round().await.unwrap();
        assert_eq!(round_data.reveal_index, 1);

        // Remove player2 (whose submission is at the current reveal index)
        state.remove_player(&player2.id).await.unwrap();

        // reveal_index should be adjusted to remain in bounds
        let round_data = state.get_current_round().await.unwrap();
        assert_eq!(round_data.reveal_order.len(), 1);
        assert_eq!(round_data.reveal_index, 0); // Adjusted to last valid index
    }

    #[tokio::test]
    async fn test_remove_player_clears_status() {
        let state = AppState::new();
        state.create_game().await;

        // Create player and set status
        let player = state.create_player().await;
        state
            .set_player_status(&player.id, PlayerSubmissionStatus::Submitted)
            .await;

        // Verify status is set
        assert_eq!(
            state.get_player_status(&player.id).await,
            PlayerSubmissionStatus::Submitted
        );

        // Remove player
        state.remove_player(&player.id).await.unwrap();

        // Status should be cleared (returns default NotSubmitted for unknown player)
        assert_eq!(
            state.get_player_status(&player.id).await,
            PlayerSubmissionStatus::NotSubmitted
        );
    }

    #[tokio::test]
    async fn test_remove_player_no_round() {
        let state = AppState::new();
        state.create_game().await;
        // Don't start a round

        // Create a player
        let player = state.create_player().await;

        // Remove should still work (no submissions to clean up)
        let result = state.remove_player(&player.id).await;
        assert!(result.is_ok());

        // Player should be gone
        assert_eq!(state.players.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_resubmit_replaces_previous_submission() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create a player
        let player = state.create_player().await;

        // First submission
        let sub1 = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "First answer".to_string(),
            )
            .await
            .unwrap();

        // Verify one submission exists
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 1);
        assert_eq!(submissions[0].original_text, "First answer");

        // Resubmit with different answer
        let sub2 = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Second answer".to_string(),
            )
            .await
            .unwrap();

        // Verify still only one submission (replacement, not addition)
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 1);
        assert_eq!(submissions[0].original_text, "Second answer");
        assert_eq!(submissions[0].id, sub2.id);
        assert_ne!(sub1.id, sub2.id); // New submission ID

        // Player can also resubmit the same text they had before
        let sub3 = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "First answer".to_string(),
            )
            .await
            .unwrap();

        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 1);
        assert_eq!(submissions[0].original_text, "First answer");
        assert_eq!(submissions[0].id, sub3.id);
    }

    // Vote challenge tests

    #[tokio::test]
    async fn test_generate_vote_challenge() {
        let state = AppState::new();

        // Initially no challenge
        assert!(state.get_vote_challenge_nonce().await.is_none());

        // Generate a challenge
        let nonce = state.generate_vote_challenge().await;
        assert_eq!(nonce.len(), 32); // 16 bytes = 32 hex chars

        // Should be stored
        let stored = state.get_vote_challenge_nonce().await;
        assert_eq!(stored, Some(nonce));
    }

    #[test]
    fn test_verify_vote_challenge_valid() {
        use sha2::{Digest, Sha256};

        let nonce = "abcdef1234567890abcdef1234567890";
        let voter_token = "voter_test_token_123";

        // Compute expected response (same algorithm as client)
        let input = format!("{}{}", nonce, voter_token);
        let hash = Sha256::digest(input.as_bytes());
        let expected_response = hex::encode(&hash[..8]); // First 8 bytes = 16 hex chars

        // Should verify successfully
        assert!(AppState::verify_vote_challenge(
            nonce,
            voter_token,
            nonce,
            &expected_response
        ));
    }

    #[test]
    fn test_verify_vote_challenge_wrong_response() {
        let nonce = "abcdef1234567890abcdef1234567890";
        let voter_token = "voter_test_token_123";
        let wrong_response = "0000000000000000"; // Wrong response

        // Should fail
        assert!(!AppState::verify_vote_challenge(
            nonce,
            voter_token,
            nonce,
            wrong_response
        ));
    }

    #[test]
    fn test_verify_vote_challenge_wrong_nonce() {
        use sha2::{Digest, Sha256};

        let correct_nonce = "abcdef1234567890abcdef1234567890";
        let wrong_nonce = "ffffffffffffffffffffffffffffffff";
        let voter_token = "voter_test_token_123";

        // Compute response with correct nonce
        let input = format!("{}{}", correct_nonce, voter_token);
        let hash = Sha256::digest(input.as_bytes());
        let response = hex::encode(&hash[..8]);

        // Verification should fail because nonce mismatch
        assert!(!AppState::verify_vote_challenge(
            correct_nonce,
            voter_token,
            wrong_nonce,
            &response
        ));
    }

    #[test]
    fn test_verify_vote_challenge_different_voter() {
        use sha2::{Digest, Sha256};

        let nonce = "abcdef1234567890abcdef1234567890";
        let voter1 = "voter_1";
        let voter2 = "voter_2";

        // Compute response for voter1
        let input = format!("{}{}", nonce, voter1);
        let hash = Sha256::digest(input.as_bytes());
        let response = hex::encode(&hash[..8]);

        // Should fail for voter2
        assert!(!AppState::verify_vote_challenge(
            nonce, voter2, nonce, &response
        ));

        // But should succeed for voter1
        assert!(AppState::verify_vote_challenge(
            nonce, voter1, nonce, &response
        ));
    }

    // Venue mode tests

    #[tokio::test]
    async fn test_venue_only_mode_toggle() {
        let state = AppState::new();
        state.create_game().await;

        // Initially disabled
        assert!(!state.is_venue_only_mode().await);

        // Enable
        state.set_venue_only_mode(true).await;
        assert!(state.is_venue_only_mode().await);

        // Disable
        state.set_venue_only_mode(false).await;
        assert!(!state.is_venue_only_mode().await);
    }

    #[test]
    fn test_venue_ip_allowed_in_hardcoded_range() {
        let state = AppState::new();

        // IP in first hardcoded range (94.45.224.0/19)
        let ip: std::net::IpAddr = "94.45.230.100".parse().unwrap();
        assert!(state.is_ip_allowed_by_venue(ip));

        // IP in second hardcoded range (151.219.0.0/16)
        let ip: std::net::IpAddr = "151.219.50.1".parse().unwrap();
        assert!(state.is_ip_allowed_by_venue(ip));

        // IPv6 in third hardcoded range (2001:67c:20a1::/48)
        let ip: std::net::IpAddr = "2001:67c:20a1:1234::1".parse().unwrap();
        assert!(state.is_ip_allowed_by_venue(ip));
    }

    #[test]
    fn test_venue_ip_not_allowed_outside_hardcoded_ranges() {
        let state = AppState::new();

        // IP outside all hardcoded ranges
        let ip: std::net::IpAddr = "192.168.1.100".parse().unwrap();
        assert!(!state.is_ip_allowed_by_venue(ip));

        let ip: std::net::IpAddr = "8.8.8.8".parse().unwrap();
        assert!(!state.is_ip_allowed_by_venue(ip));

        // IPv6 outside range
        let ip: std::net::IpAddr = "2001:db8::1".parse().unwrap();
        assert!(!state.is_ip_allowed_by_venue(ip));
    }

    #[test]
    fn test_venue_rejection_message() {
        let state = AppState::new();

        // Message contains CCH reference
        let message = state.get_venue_rejection_message();
        assert!(message.contains("CCH"));
    }
}
