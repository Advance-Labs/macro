//! Ports for phone calls.

use std::future::Future;

use macro_user_id::user_id::MacroUserIdStr;
use uuid::Uuid;

use crate::domain::models::{Call, CallError};
use crate::domain::phone::{
    IncomingPhoneCall, NewPhoneCall, PhoneContact, PhoneLeg, PhoneLegUpdate, PhoneNumber,
};

/// Persistence for phone numbers and the phone legs of live calls.
///
/// Archived phone legs are written by
/// [`CallRepository::archive_call`](super::CallRepository::archive_call),
/// which also finalizes a leg that is still live: active legs complete,
/// unanswered inbound legs are missed, and unanswered outbound legs went
/// unanswered. Read models such as
/// [`CallRecord::phone`](crate::domain::models::CallRecord::phone) carry the
/// leg of active and archived calls alike.
#[cfg_attr(test, mockall::automock)]
pub trait PhoneCallRepository: Send + Sync + 'static {
    /// Numbers assigned to `user_id`, oldest first; the first is their
    /// caller id.
    fn phone_numbers_for_user<'a>(
        &self,
        user_id: MacroUserIdStr<'a>,
    ) -> impl Future<Output = Result<Vec<PhoneNumber>, CallError>> + Send;

    /// The user `number` is assigned to, if any.
    fn phone_number_owner(
        &self,
        number: &PhoneNumber,
    ) -> impl Future<Output = Result<Option<MacroUserIdStr<'static>>, CallError>> + Send;

    /// Assign `number` to `user_id`, taking it from any previous owner.
    /// Fails with [`CallError::NotFound`] when the user does not exist.
    fn assign_phone_number<'a>(
        &self,
        number: &PhoneNumber,
        user_id: MacroUserIdStr<'a>,
    ) -> impl Future<Output = Result<(), CallError>> + Send;

    /// Unassign `number`. Returns whether it was assigned.
    fn release_phone_number(
        &self,
        number: &PhoneNumber,
    ) -> impl Future<Output = Result<bool, CallError>> + Send;

    /// Create an outbound phone call in one transaction: a standalone call
    /// owned by `call.owner` (an Owner grant, team sharing off), its phone
    /// leg, and the owner as a connected participant, since a caller is in
    /// the room from the start. Fails with [`CallError::AlreadyInCall`] when
    /// the owner is still an active participant of another call.
    fn create_outbound_phone_call(
        &self,
        call: NewPhoneCall,
    ) -> impl Future<Output = Result<Call, CallError>> + Send;

    /// Create an inbound phone call in one transaction: a standalone call
    /// owned by `call.owner` and its phone leg, with nobody connected yet.
    /// Returns `None` when `call.room_name` already has, or had, a call:
    /// webhook deliveries repeat, and a late repeat must not resurrect a call
    /// that has ended.
    fn create_inbound_phone_call(
        &self,
        call: NewPhoneCall,
    ) -> impl Future<Output = Result<Option<Call>, CallError>> + Send;

    /// The phone leg of a call that has not been archived, if it has one.
    fn get_live_phone_leg(
        &self,
        call_id: &Uuid,
    ) -> impl Future<Output = Result<Option<PhoneLeg>, CallError>> + Send;

    /// Apply `update` to a leg that is still live (dialing, ringing, or
    /// active) and return the updated leg. Returns `None` without changing
    /// anything when the leg has already ended or the call was archived, so
    /// a late report can never overwrite an outcome.
    fn update_phone_leg(
        &self,
        call_id: &Uuid,
        update: PhoneLegUpdate,
    ) -> impl Future<Output = Result<Option<PhoneLeg>, CallError>> + Send;

    /// Inbound calls ringing for `user_id`, newest first.
    fn list_ringing_phone_calls<'a>(
        &self,
        user_id: MacroUserIdStr<'a>,
    ) -> impl Future<Output = Result<Vec<IncomingPhoneCall>, CallError>> + Send;
}

/// Resolves the person behind a phone number in the CRM a user can see.
///
/// Lookups are best-effort enrichment: callers treat errors as "no match".
#[cfg_attr(test, mockall::automock)]
pub trait PhoneContactDirectory: Send + Sync + 'static {
    /// The CRM contact `user_id` would see at `number`, if any.
    fn find_contact<'a>(
        &self,
        user_id: MacroUserIdStr<'a>,
        number: &PhoneNumber,
    ) -> impl Future<Output = Result<Option<PhoneContact>, rootcause::Report>> + Send;
}

/// [`PhoneCallRepository`] for services without phone calling: nobody has a
/// number, and phone calls cannot be created.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpPhoneCallRepository;

const PHONE_CALLING_UNAVAILABLE: &str = "Phone calling isn't set up";

impl PhoneCallRepository for NoOpPhoneCallRepository {
    async fn phone_numbers_for_user(
        &self,
        _user_id: MacroUserIdStr<'_>,
    ) -> Result<Vec<PhoneNumber>, CallError> {
        Ok(Vec::new())
    }

    async fn phone_number_owner(
        &self,
        _number: &PhoneNumber,
    ) -> Result<Option<MacroUserIdStr<'static>>, CallError> {
        Ok(None)
    }

    async fn assign_phone_number(
        &self,
        _number: &PhoneNumber,
        _user_id: MacroUserIdStr<'_>,
    ) -> Result<(), CallError> {
        Err(CallError::Unavailable(
            PHONE_CALLING_UNAVAILABLE.to_string(),
        ))
    }

    async fn release_phone_number(&self, _number: &PhoneNumber) -> Result<bool, CallError> {
        Ok(false)
    }

    async fn create_outbound_phone_call(&self, _call: NewPhoneCall) -> Result<Call, CallError> {
        Err(CallError::Unavailable(
            PHONE_CALLING_UNAVAILABLE.to_string(),
        ))
    }

    async fn create_inbound_phone_call(
        &self,
        _call: NewPhoneCall,
    ) -> Result<Option<Call>, CallError> {
        Err(CallError::Unavailable(
            PHONE_CALLING_UNAVAILABLE.to_string(),
        ))
    }

    async fn get_live_phone_leg(&self, _call_id: &Uuid) -> Result<Option<PhoneLeg>, CallError> {
        Ok(None)
    }

    async fn update_phone_leg(
        &self,
        _call_id: &Uuid,
        _update: PhoneLegUpdate,
    ) -> Result<Option<PhoneLeg>, CallError> {
        Ok(None)
    }

    async fn list_ringing_phone_calls(
        &self,
        _user_id: MacroUserIdStr<'_>,
    ) -> Result<Vec<IncomingPhoneCall>, CallError> {
        Ok(Vec::new())
    }
}

/// [`PhoneContactDirectory`] for services without a CRM: no number matches.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpPhoneContactDirectory;

impl PhoneContactDirectory for NoOpPhoneContactDirectory {
    async fn find_contact(
        &self,
        _user_id: MacroUserIdStr<'_>,
        _number: &PhoneNumber,
    ) -> Result<Option<PhoneContact>, rootcause::Report> {
        Ok(None)
    }
}
