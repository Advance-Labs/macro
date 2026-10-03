# Support

Support is being implemented as a team workspace at `/support`. Tickets have a
customer-facing lifecycle independent from Macro Tasks: Open, In progress,
Waiting on customer, Waiting on team, and Resolved. Link existing team-shared
Tasks or create work from a ticket; completing or deleting a Task does not
resolve the ticket.

The workspace provides queues, customer conversations, internal notes, CRM
relationships, linked work, agent configuration and installation settings.
Customer replies must be sent from Support. The underlying Macro channel is for
internal collaboration and references; direct channel posts are not automatically
published to visitors.

Administrators configure one support agent, its public knowledge and system
prompt, and choose drafts, immediate responses, a human-response window or a
confidence threshold. A ticket can pause the agent. Human replies, resolution,
newer customer messages and handoffs suppress stale automatic answers.

Website chat uses the installation script and exact allowed origins. A visitor
session authorizes one conversation; claiming a customer's email never grants
access to their historical conversations. Private notes and internal mention
payloads are excluded from visitor replies.

Email intake uses an existing connected inbox and recipient address or alias.
Configure alias delivery in the mail provider. New mail is imported after setup;
replies use the existing threaded email send pipeline and inbox sender identity.
Legacy support auto-channels are not automatically imported. SMS is outside the
initial scope.
