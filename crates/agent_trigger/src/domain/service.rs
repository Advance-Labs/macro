//! Orchestration for evaluating one posted message.

use std::collections::HashSet;

#[cfg(test)]
mod test;

use agent_session::domain::error::Result;
use agent_session::domain::model::{AgentSession, AgentSessionId, ThreadSession};
use agent_session::domain::ports::AgentSessionRepo;
use bot_id::BotId;
use bots::domain::models::{Agent, AgentChannelScope, Bot, BotKind, BotOwner};
use entity_access::domain::models::EntityAccessReceipt;
use messages::domain::mentions::bot_mention_ids;
use messages::domain::service::MessageWrite;
use messages::domain::{events::MessagePostedMetadata, models::MessageParent};

use channel_sender::ChannelSender;
use macro_user_id::cowlike::CowLike;
use macro_user_id::user_id::MacroUserIdStr;
use macro_uuid::Uuid;

use crate::domain::broker_events::{ThreadMessageKind, TriggerDecision};
use crate::domain::thread_window::{ThreadMessage, render_transcript, thread_window};
use crate::domain::yield_event::{
    AgentSessionEventDecision, NoEventReason, PotentialTriggerEvent, yield_event,
};

/// Messages kept either side of a point where the agent spoke or was spoken to.
/// Wide enough to carry the exchange around it, narrow enough that a long
/// thread does not become mostly unrelated chatter.
const WINDOW_RADIUS: usize = 4;

/// Ceiling on transcript length, keeping the most recent messages. A judgement
/// about the newest message rarely turns on what happened dozens of messages
/// ago, and the fast model reads a bounded prompt.
const TRANSCRIPT_CAP: usize = 40;

/// Bot facts required to decide whether a mention may start an agent session.
#[cfg_attr(test, mockall::automock)]
pub trait AgentBotLookup: Send + Sync + 'static {
    /// Get an active persisted agent by bot id.
    fn get_agent(&self, bot_id: BotId) -> impl Future<Output = Result<Option<Agent>>> + Send;

    /// Get an active bot, including a fixed system bot, by id.
    fn get_bot(&self, bot_id: BotId) -> impl Future<Output = Result<Option<Bot>>> + Send;
}

/// Team-membership facts, for agents shared with their owning team.
///
/// Its own port rather than a bot fact: membership belongs to the teams
/// domain, and the trigger domain only ever asks this one question of it.
#[cfg_attr(test, mockall::automock)]
pub trait TeamMembershipLookup: Send + Sync + 'static {
    /// Check whether a user belongs to a team.
    fn user_has_team(
        &self,
        caller: MacroUserIdStr<'static>,
        team_id: Uuid,
    ) -> impl Future<Output = Result<bool>> + Send;
}

/// Channel-participation facts, for agents scoped to selected channels.
///
/// Its own port rather than a bot fact: participation belongs to the channels
/// domain, and the trigger domain only ever asks this one question of it.
#[cfg_attr(test, mockall::automock)]
pub trait ChannelParticipationLookup: Send + Sync + 'static {
    /// Check whether a bot is an active channel participant.
    fn bot_active_in_channel(
        &self,
        channel_id: Uuid,
        bot_id: BotId,
    ) -> impl Future<Output = Result<bool>> + Send;
}

/// The leading reply-target of an explicit reply: who the author answered,
/// and which message they pointed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedExplicitReply {
    /// Entity containing the targeted message.
    pub parent: MessageParent,
    /// Targeted message.
    pub target_message_id: String,
    /// Thread containing the targeted message.
    pub target_thread_id: String,
    /// Static one-line preview rendered by the reply target.
    pub display_text: String,
    /// Sender of the targeted message — who the author replied to.
    pub sender_id: String,
}

/// Extracts the leading reply-target from markdown composed as an explicit
/// reply: a `ReplyTargetNode` followed by the author's response.
#[cfg_attr(test, mockall::automock)]
pub trait ExplicitReplyExtractor: Send + Sync + 'static {
    /// The leading reply-target when this markdown is an explicit reply.
    fn extract_explicit_reply(
        &self,
        markdown: &str,
    ) -> impl Future<Output = Result<Option<ExtractedExplicitReply>>> + Send;
}

/// A current invocation capability binding the actor, parent, and thread root.
/// It is minted again when evaluating queued work, rather than persisted in events.
#[derive(Debug, Clone)]
pub struct AuthorizedInvocation {
    access: EntityAccessReceipt<MessageWrite>,
    root_id: Uuid,
}

impl AuthorizedInvocation {
    /// Bind a verified posting capability on the parent to the thread it invokes in.
    #[must_use]
    pub fn new(access: EntityAccessReceipt<MessageWrite>, root_id: Uuid) -> Self {
        Self { access, root_id }
    }

    /// The actor's verified capability on the parent.
    #[must_use]
    pub fn access(&self) -> &EntityAccessReceipt<MessageWrite> {
        &self.access
    }

    /// The thread root the invocation is bound to.
    #[must_use]
    pub fn root_id(&self) -> Uuid {
        self.root_id
    }
}

/// Reads whole threads, so an unmentioned message can be judged against the
/// conversation it landed in.
#[cfg_attr(test, mockall::automock)]
pub trait ThreadHistory: Send + Sync + 'static {
    /// Recheck that the invoking user may still write to this conversation.
    fn authorize_invocation(
        &self,
        user: &MacroUserIdStr<'static>,
        parent: &MessageParent,
        root_id: Uuid,
    ) -> impl Future<Output = Result<Option<AuthorizedInvocation>>> + Send;
    /// Live history from the exact origin covered by the invocation capability.
    fn thread_messages(
        &self,
        invocation: &AuthorizedInvocation,
    ) -> impl Future<Output = Result<Vec<ThreadMessage>>> + Send;
}

/// An agent with a live session in the thread an unmentioned message landed
/// in, as the judge is told about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateAgent {
    /// The bot whose session this is.
    pub bot_id: BotId,
    /// How the transcript names this agent. Unique among the candidates of
    /// one judgement, so the judge's answer maps back to exactly one bot.
    pub label: String,
}

/// Judges which agent, if any, an unmentioned message in a thread with live
/// agent sessions is addressed to.
#[cfg_attr(test, mockall::automock)]
pub trait ImplicitTriggerJudge: Send + Sync + 'static {
    /// The candidate the message reads as directed at, or `None` when it is
    /// conversation between people. `transcript` is the thread around the
    /// candidates' participation in it, labelled with their [`CandidateAgent::label`]s;
    /// empty when the thread could not be read.
    fn addressed_agent(
        &self,
        posted: &MessagePostedMetadata,
        transcript: &str,
        candidates: &[CandidateAgent],
    ) -> impl Future<Output = Result<Option<BotId>>> + Send;
}

/// A live session an unmentioned message might be for, with the display name
/// of the bot that runs it.
#[derive(Debug, Clone)]
struct LiveAgent {
    session: AgentSession,
    name: String,
}

/// Looks up the session context for a message and evaluates its trigger rule.
pub struct AgentTriggerService<Repo, Bots, Teams, Channels, Replies, Judge, History> {
    sessions: Repo,
    bots: Bots,
    teams: Teams,
    channels: Channels,
    replies: Replies,
    judge: Judge,
    history: History,
}

impl<Repo, Bots, Teams, Channels, Replies, Judge, History>
    AgentTriggerService<Repo, Bots, Teams, Channels, Replies, Judge, History>
where
    Repo: AgentSessionRepo,
    Bots: AgentBotLookup,
    Teams: TeamMembershipLookup,
    Channels: ChannelParticipationLookup,
    Replies: ExplicitReplyExtractor,
    Judge: ImplicitTriggerJudge,
    History: ThreadHistory,
{
    /// Creates a trigger service backed by session, bot, membership, and
    /// participation lookups.
    pub const fn new(
        sessions: Repo,
        bots: Bots,
        teams: Teams,
        channels: Channels,
        replies: Replies,
        judge: Judge,
        history: History,
    ) -> Self {
        Self {
            sessions,
            bots,
            teams,
            channels,
            replies,
            judge,
            history,
        }
    }

    /// Whether `posted` may address `bot_id` under the bot's current scope.
    async fn agent_is_available(
        &self,
        posted: &MessagePostedMetadata,
        bot_id: BotId,
    ) -> Result<bool> {
        Ok(self.available_agent(posted, bot_id).await?.is_some())
    }

    /// The bot `posted` may address as `bot_id` under the bot's current
    /// scope, or `None` when it may not.
    ///
    /// System agents are global. Persisted all-channel agents are private to
    /// their owner or shared with their owning team. Selected agents and
    /// legacy agent-backed bots use explicit channel participation.
    async fn available_agent(
        &self,
        posted: &MessagePostedMetadata,
        bot_id: BotId,
    ) -> Result<Option<Bot>> {
        let Some(caller) = posted.sender.as_user().cloned().map(CowLike::into_owned) else {
            return Ok(None);
        };

        if let Some(agent) = self.bots.get_agent(bot_id).await? {
            if !agent.bot.has_agent {
                return Ok(None);
            }
            // Channel selection restricts channel placement. Discussion invocation
            // requires ownership or team membership and is independently bounded
            // by the invoking user's parent access at execution time.
            let allowed = if posted.parent.is_discussion() {
                self.owner_allows(&caller, agent.bot.owner.as_ref()).await?
            } else {
                let MessageParent::Channel(channel_id) = posted.parent else {
                    unreachable!()
                };
                match agent.channel_scope {
                    AgentChannelScope::All => match &agent.bot.owner {
                        Some(BotOwner::User { user_id }) => user_id == caller.as_ref(),
                        Some(BotOwner::Team { team_id }) => {
                            self.teams.user_has_team(caller, *team_id).await?
                        }
                        None => false,
                    },
                    AgentChannelScope::Selected => {
                        self.channels
                            .bot_active_in_channel(channel_id, bot_id)
                            .await?
                    }
                }
            };
            return Ok(allowed.then_some(agent.bot));
        }

        let Some(bot) = self.bots.get_bot(bot_id).await? else {
            return Ok(None);
        };
        if !bot.has_agent {
            return Ok(None);
        }
        let allowed = match bot.kind {
            BotKind::System => true,
            BotKind::Owned => match &posted.parent {
                MessageParent::Channel(channel_id) => {
                    self.channels
                        .bot_active_in_channel(*channel_id, bot_id)
                        .await?
                }
                MessageParent::Document(_) | MessageParent::Initiative(_) => {
                    self.owner_allows(&caller, bot.owner.as_ref()).await?
                }
            },
        };
        Ok(allowed.then_some(bot))
    }

    async fn owner_allows(
        &self,
        caller: &MacroUserIdStr<'static>,
        owner: Option<&BotOwner>,
    ) -> Result<bool> {
        match owner {
            Some(BotOwner::User { user_id }) => Ok(user_id == caller.as_ref()),
            Some(BotOwner::Team { team_id }) => {
                self.teams.user_has_team(caller.clone(), *team_id).await
            }
            None => Ok(false),
        }
    }

    /// Evaluates a posted message for every mentioned bot.
    #[tracing::instrument(err, skip(self, posted), fields(
        parent = ?posted.parent,
        message_id = %posted.message_id,
        thread_id = ?posted.thread_id,
        message.scope = tracing::field::Empty,
        agent.mention.bot_count = tracing::field::Empty,
    ))]
    pub async fn evaluate(&self, posted: &MessagePostedMetadata) -> Result<Vec<TriggerDecision>> {
        let Some(user) = posted.sender.as_user().cloned().map(CowLike::into_owned) else {
            return Ok(Vec::new());
        };
        let Some(invocation) = self
            .history
            .authorize_invocation(&user, &posted.parent, posted.root_id())
            .await?
        else {
            return Ok(Vec::new());
        };
        let mut mentioned = bot_mention_ids(&posted.mentions);
        mentioned.sort_by_key(ToString::to_string);
        tracing::Span::current().record(
            "message.scope",
            if posted.thread_id.is_some() {
                "thread"
            } else {
                "root"
            },
        );
        tracing::Span::current().record("agent.mention.bot_count", mentioned.len());
        let mut seen_sessions = HashSet::new();
        let mut events = Vec::new();

        if mentioned.is_empty() {
            if let Some(event) = self.evaluate_bot(posted, None, &mut seen_sessions).await? {
                events.push(event);
            } else if let Some(event) = self.evaluate_implicit(posted, &invocation).await? {
                events.push(event);
            }
            return Ok(events);
        }

        for bot_id in mentioned {
            if let Some(event) = self
                .evaluate_bot(posted, Some(bot_id), &mut seen_sessions)
                .await?
            {
                events.push(event);
            }
        }

        Ok(events)
    }

    #[tracing::instrument(
        err,
        skip(self, posted, seen_sessions),
        fields(
            parent = ?posted.parent,
            message_id = %posted.message_id,
            bot_id = ?mentioned_bot,
            agent.trigger.outcome = tracing::field::Empty,
        )
    )]
    async fn evaluate_bot(
        &self,
        posted: &MessagePostedMetadata,
        mentioned_bot: Option<BotId>,
        seen_sessions: &mut HashSet<AgentSessionId>,
    ) -> Result<Option<TriggerDecision>> {
        let existing = self
            .sessions
            .find_for_thread(posted.thread_id, mentioned_bot)
            .await?;
        if let ThreadSession::CreatedFromThread(session) = &existing
            && (session.thread_parent.as_ref() != Some(&posted.parent)
                || session.thread_id != posted.thread_id)
        {
            return Ok(None);
        }
        if let Some(session_id) = session_id(&existing)
            && !seen_sessions.insert(session_id)
        {
            let reason = NoEventReason::DuplicateSession { session_id };
            tracing::Span::current().record("agent.trigger.outcome", reason.as_ref());
            log_no_event(posted, mentioned_bot, reason);
            return Ok(None);
        }
        let bot = match &existing {
            ThreadSession::CreatedFromThread(session) => Some(session.bot_id),
            ThreadSession::None => mentioned_bot,
        };
        let available = match bot {
            Some(bot_id) => self.agent_is_available(posted, bot_id).await?,
            None => false,
        };

        let message = PotentialTriggerEvent::Thread {
            posted,
            existing: &existing,
            mentioned_bot,
        };
        match yield_event(&message, available) {
            AgentSessionEventDecision::Event(event) => {
                let outcome = match existing {
                    ThreadSession::None => "top_level_mentioned",
                    ThreadSession::CreatedFromThread(_) => "mention_thread",
                };
                tracing::Span::current().record("agent.trigger.outcome", outcome);
                Ok(Some(event))
            }
            AgentSessionEventDecision::NoEvent(reason) => {
                tracing::Span::current().record("agent.trigger.outcome", reason.as_ref());
                log_no_event(posted, mentioned_bot, reason);
                Ok(None)
            }
        }
    }

    /// Evaluates an unmentioned message against the sessions rooted at its
    /// thread: an explicit reply that targets a live session's bot, or that
    /// session's originating message, is forwarded outright. Anything else
    /// only when the judge reads it as addressed to one of the live agents.
    ///
    /// A thread carries at most one session per bot, but any number of bots:
    /// a person can bring a second agent into a thread the first is already
    /// working in. An extracted reply names who and which message it answers,
    /// so it picks among them on its face. Otherwise the judge is shown every
    /// live agent, by name, and says which one - if any - the message is for.
    ///
    /// Extractor and judge failures are treated as "no" rather than propagated:
    /// implicit triggering is best-effort, and an outage must not wedge the
    /// message stream or fabricate forwards.
    async fn evaluate_implicit(
        &self,
        posted: &MessagePostedMetadata,
        invocation: &AuthorizedInvocation,
    ) -> Result<Option<TriggerDecision>> {
        let Some(thread_id) = posted.thread_id else {
            return Ok(None);
        };
        // Only a user implicitly addresses an agent; bot traffic must always
        // mention explicitly, or bots would relay each other forever.
        if posted.sender.as_user().is_none() {
            return Ok(None);
        }
        let mut live = Vec::new();
        for session in self.sessions.find_all_for_thread(thread_id).await? {
            if session.thread_parent.as_ref() == Some(&posted.parent)
                && session.thread_id == Some(thread_id)
                && let Some(bot) = self.available_agent(posted, session.bot_id).await?
            {
                live.push(LiveAgent {
                    session,
                    name: bot.name,
                });
            }
        }
        if live.is_empty() {
            return Ok(None);
        }

        if let Some(session) = self.explicit_reply_session(posted, &live).await {
            // The reply-target says who it answers on its face, so it needs no
            // thread read at all.
            return Ok(Some(thread_event(
                session,
                ThreadMessageKind::ExplicitReply,
                posted,
            )));
        }

        let candidates = candidate_agents(&live);
        let transcript = self
            .transcript(posted, invocation, &live, &candidates)
            .await;
        if let Some(bot_id) = self.addressed_agent(posted, &transcript, &candidates).await
            && let Some(agent) = live.iter().find(|agent| agent.session.bot_id == bot_id)
        {
            return Ok(Some(thread_event(
                agent.session.clone(),
                ThreadMessageKind::Inferred,
                posted,
            )));
        }

        log_no_event(
            posted,
            None,
            NoEventReason::NotAddressedToAgent {
                candidates: live.len(),
            },
        );
        Ok(None)
    }

    /// The live session the extracted reply-target names: either its bot, or
    /// uniquely its originating message.
    async fn explicit_reply_session(
        &self,
        posted: &MessagePostedMetadata,
        live: &[LiveAgent],
    ) -> Option<AgentSession> {
        let reply = self
            .replies
            .extract_explicit_reply(&posted.content)
            .await
            .inspect_err(|error| {
                tracing::warn!(
                    error = ?error,
                    "explicit reply extraction failed; treating as not a reply"
                );
            })
            .ok()
            .flatten()?;
        if reply.parent != posted.parent || reply.target_thread_id != posted.root_id().to_string() {
            return None;
        }
        let candidates: Vec<&AgentSession> = live.iter().map(|agent| &agent.session).collect();
        session_targeted_by_reply(&candidates, &reply).cloned()
    }

    /// The thread around the points where the live agents took part: their
    /// own messages, and the message being judged, each with [`WINDOW_RADIUS`]
    /// messages of surrounding conversation.
    ///
    /// An unreadable thread yields an empty transcript rather than an error, so
    /// the judge still rules on the message itself instead of the whole path
    /// wedging on a thread read.
    async fn transcript(
        &self,
        posted: &MessagePostedMetadata,
        invocation: &AuthorizedInvocation,
        live: &[LiveAgent],
        candidates: &[CandidateAgent],
    ) -> String {
        let messages = match self.history.thread_messages(invocation).await {
            Ok(messages) => messages,
            Err(error) => {
                tracing::warn!(error = ?error, "thread read failed; judging without thread context");
                return String::new();
            }
        };
        let agents: Vec<_> = live
            .iter()
            .map(|agent| agent.session.bot_id.into_storage_id())
            .collect();
        let anchors: Vec<Uuid> = messages
            .iter()
            .filter(|message| {
                message.id == posted.message_id
                    || message.sender.as_bot().is_some_and(|bot| {
                        agents.iter().any(|agent| agent.as_ref() == bot.as_ref())
                    })
            })
            .map(|message| message.id)
            .collect();

        render_transcript(
            &thread_window(&messages, &anchors, WINDOW_RADIUS, TRANSCRIPT_CAP),
            candidates,
        )
    }

    /// The judge's pick, held to the candidates it was offered: a bot it was
    /// not asked about is no answer at all.
    async fn addressed_agent(
        &self,
        posted: &MessagePostedMetadata,
        transcript: &str,
        candidates: &[CandidateAgent],
    ) -> Option<BotId> {
        let picked = self
            .judge
            .addressed_agent(posted, transcript, candidates)
            .await
            .inspect_err(|error| {
                tracing::warn!(error = ?error, "implicit trigger judge failed; treating as not addressed");
            })
            .ok()
            .flatten()?;
        if candidates
            .iter()
            .any(|candidate| candidate.bot_id == picked)
        {
            Some(picked)
        } else {
            tracing::warn!(
                bot_id = %picked,
                "implicit trigger judge named a bot it was not offered; treating as not addressed"
            );
            None
        }
    }
}

/// The live agents as the judge is told about them, each under a label that
/// is unique among them. Labels are display names; two bots that share a name
/// are told apart by a counter, since the judge can only answer with a label.
fn candidate_agents(live: &[LiveAgent]) -> Vec<CandidateAgent> {
    let mut labels: HashSet<String> = HashSet::new();
    live.iter()
        .map(|agent| {
            let mut label = agent.name.trim().to_owned();
            if label.is_empty() {
                label = "agent".to_owned();
            }
            let base = label.clone();
            let mut suffix = 2_usize;
            while !labels.insert(label.clone()) {
                label = format!("{base} #{suffix}");
                suffix += 1;
            }
            CandidateAgent {
                bot_id: agent.session.bot_id,
                label,
            }
        })
        .collect()
}

fn log_no_event(
    posted: &MessagePostedMetadata,
    mentioned_bot: Option<BotId>,
    reason: NoEventReason,
) {
    tracing::debug!(
        message_id = %posted.message_id,
        ?mentioned_bot,
        ?reason,
        "agent trigger emitted no event"
    );
}

fn session_id(session: &ThreadSession) -> Option<AgentSessionId> {
    match session {
        ThreadSession::CreatedFromThread(session) => Some(session.id),
        ThreadSession::None => None,
    }
}

fn thread_event(
    session: AgentSession,
    kind: ThreadMessageKind,
    posted: &MessagePostedMetadata,
) -> TriggerDecision {
    TriggerDecision::Existing {
        bot_id: session.bot_id,
        session_id: session.id,
        kind,
        message: posted.clone(),
    }
}

/// The live session the reply-target names: its bot as addressee, or uniquely
/// the message that opened the session.
fn session_targeted_by_reply<'a>(
    candidates: &[&'a AgentSession],
    reply: &ExtractedExplicitReply,
) -> Option<&'a AgentSession> {
    if let Some(session) = session_named_by_bot(candidates, reply) {
        return Some(session);
    }
    session_named_by_originating_message(candidates, reply)
}

fn session_named_by_bot<'a>(
    candidates: &[&'a AgentSession],
    reply: &ExtractedExplicitReply,
) -> Option<&'a AgentSession> {
    let sender = ChannelSender::parse_from_str(&reply.sender_id).ok()?;
    let bot_id = sender.as_bot()?.bot_id();
    candidates
        .iter()
        .copied()
        .find(|session| session.bot_id == bot_id)
}

fn session_named_by_originating_message<'a>(
    candidates: &[&'a AgentSession],
    reply: &ExtractedExplicitReply,
) -> Option<&'a AgentSession> {
    let target = Uuid::parse_str(&reply.target_message_id).ok()?;
    let mut matches = candidates
        .iter()
        .copied()
        .filter(|session| session.originating_message_id == Some(target));
    let session = matches.next()?;
    matches.next().is_none().then_some(session)
}
