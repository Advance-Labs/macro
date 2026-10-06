//! Persistence for phone numbers and phone legs.
//!
//! A phone leg has a live row in `call_phone_legs` while its call is active
//! and an archived row in `call_record_phone_legs` written by the archive
//! transaction (see [`archive_phone_leg`]).

use sqlx::{PgConnection, Postgres, Transaction};

use super::*;
use crate::domain::phone::{
    IncomingPhoneCall, NewPhoneCall, PhoneCallDirection, PhoneCallStatus, PhoneContact, PhoneLeg,
    PhoneLegUpdate, PhoneNumber,
};
use crate::domain::ports::phone::PhoneCallRepository;

/// Foreign-key violation, e.g. assigning a number to a user that does not exist.
const FOREIGN_KEY_VIOLATION: &str = "23503";

/// A phone leg row from either table, before validation.
struct PhoneLegRow {
    call_id: Uuid,
    direction: String,
    remote_number: String,
    local_number: Option<String>,
    participant_identity: String,
    status: String,
    crm_contact_id: Option<Uuid>,
    contact_name: Option<String>,
    answered_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
}

impl PhoneLegRow {
    fn into_leg(self) -> Result<(Uuid, PhoneLeg), sqlx::Error> {
        Ok((
            self.call_id,
            PhoneLeg {
                direction: self.direction.parse().map_err(decode_error)?,
                remote_number: stored_number(&self.remote_number)?,
                local_number: self.local_number.as_deref().map(stored_number).transpose()?,
                participant_identity: self.participant_identity,
                status: self.status.parse().map_err(decode_error)?,
                contact: self.crm_contact_id.map(|contact_id| PhoneContact {
                    contact_id,
                    name: self.contact_name,
                }),
                answered_at: self.answered_at,
                ended_at: self.ended_at,
            },
        ))
    }
}

/// A stored value that does not decode into the domain type.
fn decode_error(error: impl std::error::Error + Send + Sync + 'static) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(error))
}

fn stored_number(value: &str) -> Result<PhoneNumber, sqlx::Error> {
    PhoneNumber::from_e164(value).map_err(decode_error)
}

/// The phone leg of an active call, read inside the caller's transaction.
pub(super) async fn fetch_live_leg(
    conn: &mut PgConnection,
    call_id: &Uuid,
) -> Result<Option<PhoneLeg>, sqlx::Error> {
    sqlx::query_as!(
        PhoneLegRow,
        r#"
        SELECT call_id, direction, remote_number, local_number, participant_identity, status,
               crm_contact_id, contact_name, answered_at, ended_at
        FROM call_phone_legs
        WHERE call_id = $1
        "#,
        call_id,
    )
    .fetch_optional(conn)
    .await?
    .map(|row| row.into_leg().map(|(_, leg)| leg))
    .transpose()
}

/// The phone leg of an archived call, read inside the caller's transaction.
pub(super) async fn fetch_archived_leg(
    conn: &mut PgConnection,
    call_record_id: &Uuid,
) -> Result<Option<PhoneLeg>, sqlx::Error> {
    sqlx::query_as!(
        PhoneLegRow,
        r#"
        SELECT call_record_id AS call_id, direction, remote_number, local_number,
               participant_identity, status, crm_contact_id, contact_name, answered_at,
               ended_at AS "ended_at?"
        FROM call_record_phone_legs
        WHERE call_record_id = $1
        "#,
        call_record_id,
    )
    .fetch_optional(conn)
    .await?
    .map(|row| row.into_leg().map(|(_, leg)| leg))
    .transpose()
}

/// Phone legs of a page of active and archived calls, keyed by call id.
pub(super) async fn fetch_legs(
    pool: &PgPool,
    active_ids: &[Uuid],
    archived_ids: &[Uuid],
) -> Result<HashMap<Uuid, PhoneLeg>, sqlx::Error> {
    if active_ids.is_empty() && archived_ids.is_empty() {
        return Ok(HashMap::new());
    }
    sqlx::query_as!(
        PhoneLegRow,
        r#"
        SELECT call_id AS "call_id!", direction AS "direction!", remote_number AS "remote_number!",
               local_number, participant_identity AS "participant_identity!", status AS "status!",
               crm_contact_id, contact_name, answered_at, ended_at
        FROM call_phone_legs
        WHERE call_id = ANY($1)
        UNION ALL
        SELECT call_record_id, direction, remote_number, local_number, participant_identity,
               status, crm_contact_id, contact_name, answered_at, ended_at
        FROM call_record_phone_legs
        WHERE call_record_id = ANY($2)
        "#,
        active_ids,
        archived_ids,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(PhoneLegRow::into_leg)
    .collect()
}

/// The external numbers on an archived call.
pub(super) async fn fetch_archived_remote_numbers(
    pool: &PgPool,
    call_record_id: &Uuid,
) -> Result<Vec<PhoneNumber>, sqlx::Error> {
    let numbers = sqlx::query_scalar!(
        "SELECT remote_number FROM call_record_phone_legs WHERE call_record_id = $1",
        call_record_id,
    )
    .fetch_all(pool)
    .await?;
    // A malformed stored number names nobody we can match; skip it.
    Ok(numbers
        .iter()
        .filter_map(|number| PhoneNumber::from_e164(number).ok())
        .collect())
}

/// Move an active call's phone leg to the archive, inside the archive
/// transaction. A leg that is still live gets its final outcome here: the
/// call is ending, so answered calls complete and unanswered calls are
/// missed (inbound) or went unanswered (outbound).
pub(super) async fn archive_phone_leg(
    tx: &mut Transaction<'_, Postgres>,
    call_id: &Uuid,
    archived_at: DateTime<Utc>,
) -> Result<(), CallError> {
    let Some(leg) = sqlx::query!(
        r#"
        SELECT direction, status, sip_call_id, created_at
        FROM call_phone_legs
        WHERE call_id = $1
        "#,
        call_id,
    )
    .fetch_optional(tx.as_mut())
    .await?
    else {
        return Ok(());
    };
    let direction: PhoneCallDirection = leg.direction.parse().map_err(decode_error)?;
    let status = leg
        .status
        .parse::<PhoneCallStatus>()
        .map_err(decode_error)?
        .concluded(direction);
    sqlx::query!(
        r#"
        INSERT INTO call_record_phone_legs (
            call_record_id, direction, remote_number, local_number, participant_identity,
            status, crm_contact_id, contact_name, sip_call_id, answered_at, ended_at, created_at
        )
        SELECT call_id, direction, remote_number, local_number, participant_identity,
               $2, crm_contact_id, contact_name, sip_call_id, answered_at,
               COALESCE(ended_at, $3), created_at
        FROM call_phone_legs
        WHERE call_id = $1
        "#,
        call_id,
        status.as_str(),
        archived_at,
    )
    .execute(tx.as_mut())
    .await?;
    Ok(())
}

/// Create a standalone call owned by `call.owner`, with its phone leg: an
/// Owner grant and no team sharing, like any channel-less call.
async fn insert_phone_call(
    tx: &mut Transaction<'_, Postgres>,
    call: &NewPhoneCall,
) -> Result<Call, CallError> {
    let share_permission_id = Uuid::now_v7().to_string();
    sqlx::query!(
        r#"INSERT INTO "SharePermission" (id, "linkShare", "linkShareAccessLevel", "createdAt", "updatedAt")
           VALUES ($1, NULL, NULL, NOW(), NOW())"#,
        share_permission_id,
    )
    .execute(tx.as_mut())
    .await?;
    entity_access_db_utils::insert_entity_access_row(
        tx,
        &call.call_id,
        entity_access_db_utils::EntityType::Call,
        call.owner.as_ref(),
        entity_access_db_utils::EntityAccessSourceType::User,
        entity_access_db_utils::AccessLevel::Owner,
    )
    .await?;
    let created = sqlx::query!(
        r#"
        INSERT INTO calls (id, channel_id, room_name, created_by, share_permission_id, share_with_team, meeting_id)
        VALUES ($1, NULL, $2, $3, $4, FALSE, NULL)
        RETURNING id, channel_id, room_name, created_by, created_at, egress_id
        "#,
        call.call_id,
        call.room_name,
        call.owner.as_ref(),
        share_permission_id,
    )
    .fetch_one(tx.as_mut())
    .await?;
    let leg = &call.leg;
    sqlx::query!(
        r#"
        INSERT INTO call_phone_legs (
            call_id, direction, remote_number, local_number, participant_identity, status,
            crm_contact_id, contact_name, sip_call_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        call.call_id,
        leg.direction.as_str(),
        leg.remote_number.as_str(),
        leg.local_number.as_ref().map(PhoneNumber::as_str),
        leg.participant_identity,
        leg.status.as_str(),
        leg.contact.as_ref().map(|contact| contact.contact_id),
        leg.contact.as_ref().and_then(|contact| contact.name.as_deref()),
        leg.sip_call_id,
    )
    .execute(tx.as_mut())
    .await?;
    Ok(Call {
        id: created.id,
        channel_id: created.channel_id,
        room_name: created.room_name,
        created_by: created.created_by,
        created_at: created.created_at,
        egress_id: created.egress_id,
    })
}

impl PhoneCallRepository for PgCallRepo {
    #[tracing::instrument(err, skip(self))]
    async fn phone_numbers_for_user(
        &self,
        user_id: MacroUserIdStr<'_>,
    ) -> Result<Vec<PhoneNumber>, CallError> {
        sqlx::query_scalar!(
            r#"
            SELECT phone_number FROM phone_numbers
            WHERE user_id = $1
            ORDER BY created_at, phone_number
            "#,
            user_id.as_ref(),
        )
        .fetch_all(&self.pool)
        .await?
        .iter()
        .map(|number| stored_number(number).map_err(CallError::from))
        .collect()
    }

    #[tracing::instrument(err, skip(self))]
    async fn phone_number_owner(
        &self,
        number: &PhoneNumber,
    ) -> Result<Option<MacroUserIdStr<'static>>, CallError> {
        sqlx::query_scalar!(
            "SELECT user_id FROM phone_numbers WHERE phone_number = $1",
            number.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|user_id| {
            MacroUserIdStr::try_from(user_id).map_err(|error| CallError::Internal(error.into()))
        })
        .transpose()
    }

    #[tracing::instrument(err, skip(self))]
    async fn assign_phone_number(
        &self,
        number: &PhoneNumber,
        user_id: MacroUserIdStr<'_>,
    ) -> Result<(), CallError> {
        sqlx::query!(
            r#"
            INSERT INTO phone_numbers (phone_number, user_id)
            VALUES ($1, $2)
            ON CONFLICT (phone_number) DO UPDATE
            SET user_id = EXCLUDED.user_id, created_at = now()
            WHERE phone_numbers.user_id IS DISTINCT FROM EXCLUDED.user_id
            "#,
            number.as_str(),
            user_id.as_ref(),
        )
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if error.as_database_error().and_then(|db| db.code()).as_deref()
                == Some(FOREIGN_KEY_VIOLATION)
            {
                CallError::NotFound(format!("user {user_id}"))
            } else {
                error.into()
            }
        })?;
        Ok(())
    }

    #[tracing::instrument(err, skip(self))]
    async fn release_phone_number(&self, number: &PhoneNumber) -> Result<bool, CallError> {
        Ok(sqlx::query!(
            "DELETE FROM phone_numbers WHERE phone_number = $1",
            number.as_str(),
        )
        .execute(&self.pool)
        .await?
        .rows_affected()
            > 0)
    }

    #[tracing::instrument(err, skip(self, call), fields(call_id = %call.call_id))]
    async fn create_outbound_phone_call(&self, call: NewPhoneCall) -> Result<Call, CallError> {
        let mut tx = self.pool.begin().await?;
        let created = insert_phone_call(&mut tx, &call).await?;
        sqlx::query!(
            "INSERT INTO call_participants (call_id, user_id) VALUES ($1, $2)",
            call.call_id,
            call.owner.as_ref(),
        )
        .execute(tx.as_mut())
        .await
        .map_err(|error| match classify_add_participant_err(error) {
            AddParticipantError::UserAlreadyActive => {
                CallError::AlreadyInCall("another call".to_string())
            }
            AddParticipantError::Repository(error) => CallError::Internal(error),
        })?;
        tx.commit().await?;
        Ok(created)
    }

    #[tracing::instrument(err, skip(self, call), fields(call_id = %call.call_id))]
    async fn create_inbound_phone_call(
        &self,
        call: NewPhoneCall,
    ) -> Result<Option<Call>, CallError> {
        let mut tx = self.pool.begin().await?;
        // Repeated deliveries of one webhook race to create the same call.
        sqlx::query!(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            call.room_name,
        )
        .execute(tx.as_mut())
        .await?;
        let room_used = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (SELECT 1 FROM calls WHERE room_name = $1)
                OR EXISTS (SELECT 1 FROM call_records WHERE room_name = $1) AS "used!"
            "#,
            call.room_name,
        )
        .fetch_one(tx.as_mut())
        .await?;
        if room_used {
            return Ok(None);
        }
        let created = insert_phone_call(&mut tx, &call).await?;
        tx.commit().await?;
        Ok(Some(created))
    }

    #[tracing::instrument(err, skip(self))]
    async fn get_live_phone_leg(&self, call_id: &Uuid) -> Result<Option<PhoneLeg>, CallError> {
        let mut conn = self.pool.acquire().await?;
        Ok(fetch_live_leg(&mut conn, call_id).await?)
    }

    #[tracing::instrument(err, skip(self))]
    async fn update_phone_leg(
        &self,
        call_id: &Uuid,
        update: PhoneLegUpdate,
    ) -> Result<Option<PhoneLeg>, CallError> {
        sqlx::query_as!(
            PhoneLegRow,
            r#"
            UPDATE call_phone_legs
            SET status = $2,
                answered_at = COALESCE($3, answered_at),
                ended_at = COALESCE($4, ended_at),
                sip_call_id = COALESCE($5, sip_call_id)
            WHERE call_id = $1 AND status IN ('dialing', 'ringing', 'active')
            RETURNING call_id, direction, remote_number, local_number, participant_identity,
                      status, crm_contact_id, contact_name, answered_at, ended_at
            "#,
            call_id,
            update.status.as_str(),
            update.answered_at,
            update.ended_at,
            update.sip_call_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|row| row.into_leg().map(|(_, leg)| leg))
        .transpose()
        .map_err(CallError::from)
    }

    #[tracing::instrument(err, skip(self))]
    async fn list_ringing_phone_calls(
        &self,
        user_id: MacroUserIdStr<'_>,
    ) -> Result<Vec<IncomingPhoneCall>, CallError> {
        sqlx::query!(
            r#"
            SELECT l.call_id, l.remote_number, l.local_number, l.crm_contact_id, l.contact_name,
                   c.created_at
            FROM call_phone_legs l
            JOIN calls c ON c.id = l.call_id
            WHERE c.created_by = $1 AND l.direction = 'inbound' AND l.status = 'ringing'
            ORDER BY c.created_at DESC
            "#,
            user_id.as_ref(),
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| -> Result<IncomingPhoneCall, CallError> {
            Ok(IncomingPhoneCall {
                call_id: row.call_id,
                from: stored_number(&row.remote_number)?,
                to: row.local_number.as_deref().map(stored_number).transpose()?,
                contact: row.crm_contact_id.map(|contact_id| PhoneContact {
                    contact_id,
                    name: row.contact_name,
                }),
                started_at: row.created_at,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod test;
