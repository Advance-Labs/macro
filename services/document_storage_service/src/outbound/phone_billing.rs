//! Phone minutes in usage billing.
//!
//! Each connected call's minutes are recorded as counted `phone_call` usage
//! under the `pstn` model, priced per minute in `ai_pricing`. The payer's
//! existing allowance, credits, overage and Stripe settlement then pay for
//! them: phone seats include a monthly bucket of minutes, and minutes past it
//! spend credits or usage billing like any other usage.

#[cfg(test)]
mod test;

use std::sync::Arc;

use ai_billing::outbound::{
    HttpPaymentGateway, HttpSettlementTrigger, PgBillingRepo, PgUsageReader,
    RolesTeamsEntitlementSource, SettlingUsageRecorder,
};
use ai_billing::{
    AiAdmissionError, AiAdmissionService, AiPricing, AiUsageBilling, AiUsageEnforcement,
    BillingAdmissionService, DenyReason, domain::BillingServiceImpl,
};
use ai_usage::{AiFeature, UsageContext, UsageRecorder};
use authentication_service_client::AuthServiceClient;
use macro_user_id::user_id::MacroUserIdStr;
use roles_and_permissions::domain::service::UserRolesAndPermissionsServiceImpl;
use roles_and_permissions::outbound::pgpool::MacroDB;
use sqlx::PgPool;
use teams::outbound::team_repo::TeamRepositoryImpl;

use call::domain::ports::phone::{PhoneBilling, PhoneBillingError, PhoneBillingFuture, PhoneUsage};

/// The `ai_pricing` model phone minutes are priced under.
pub const PHONE_USAGE_MODEL: &str = "pstn";

/// Compose phone billing for this host. Calls are admitted against the
/// owner's phone plan and minutes are counted under `enforcement`. With an
/// authentication-service client, recorded minutes past a payer's allowance
/// also ask that service to settle (when `settlement` is enabled), exactly
/// like AI usage recorded by the cognition service.
pub fn compose(
    pool: PgPool,
    enforcement: AiUsageEnforcement,
    pricing: AiPricing,
    settlement: AiUsageBilling,
    auth: Option<Arc<AuthServiceClient>>,
) -> Arc<dyn PhoneBilling> {
    let permissions = UserRolesAndPermissionsServiceImpl::new(
        MacroDB::new(pool.clone()),
        MacroDB::new(pool.clone()),
    );
    let teams = TeamRepositoryImpl::new(pool.clone());
    let Some(auth) = auth else {
        return Arc::new(AiPhoneBilling::new(
            ai_billing::composition::admission_service(
                pool.clone(),
                permissions,
                teams,
                enforcement,
                pricing,
            ),
            ai_usage::pg_recorder_with_enforcement(pool, enforcement),
        ));
    };
    let billing = Arc::new(
        BillingServiceImpl::new(
            RolesTeamsEntitlementSource::new(permissions, teams),
            PgUsageReader::new(pool.clone()),
            PgBillingRepo::new(pool.clone(), pricing),
            HttpPaymentGateway::new(auth.clone()),
            pricing,
        )
        .with_enforcement(enforcement),
    );
    let admission = Arc::new(BillingAdmissionService::new(billing.clone(), enforcement));
    let recorder = Arc::new(SettlingUsageRecorder::new(
        Arc::new(
            ai_usage::domain::service::UsageServiceImpl::new(ai_usage::outbound::PgUsageRepo::new(
                pool.clone(),
            ))
            .with_enforcement(enforcement),
        ),
        billing,
        HttpSettlementTrigger::new(auth),
        settlement,
    ));
    Arc::new(AiPhoneBilling::new(
        admission,
        ai_usage::with_tracking(recorder, ai_usage::pg_tracking(pool)),
    ))
}

/// [`PhoneBilling`] through AI usage admission and recording.
pub struct AiPhoneBilling {
    admission: Arc<dyn AiAdmissionService>,
    recorder: Arc<dyn UsageRecorder>,
}

impl AiPhoneBilling {
    /// Admit calls with `admission` and record minutes with `recorder`. Both
    /// must share the host's usage enforcement policy.
    pub fn new(admission: Arc<dyn AiAdmissionService>, recorder: Arc<dyn UsageRecorder>) -> Self {
        Self {
            admission,
            recorder,
        }
    }
}

impl PhoneBilling for AiPhoneBilling {
    fn admit<'a>(
        &'a self,
        user: &'a MacroUserIdStr<'_>,
    ) -> PhoneBillingFuture<'a, Result<(), PhoneBillingError>> {
        Box::pin(async move {
            self.admission
                .admit(user, AiFeature::PhoneCall)
                .await
                .map_err(|error| match error {
                    AiAdmissionError::Denied(reason) => PhoneBillingError::Denied {
                        code: reason.code(),
                        message: phone_denial_message(reason),
                    },
                    AiAdmissionError::Unavailable => PhoneBillingError::Unavailable,
                })
        })
    }

    fn record(&self, usage: PhoneUsage) {
        self.recorder.record(
            UsageContext::new(AiFeature::PhoneCall, usage.user)
                .with_entity(Some(usage.call_id))
                .into_audio_event(PHONE_USAGE_MODEL.to_string(), usage.billed),
        );
    }
}

/// The overage reasons are shared with AI; explain them in terms of calling.
fn phone_denial_message(reason: DenyReason) -> &'static str {
    match reason {
        DenyReason::OverageLimitReached => {
            "You've reached your usage billing limit for this period. Raise the limit or add credits to keep calling."
        }
        DenyReason::OveragePaymentFailed => {
            "Your last usage charge didn't go through. Update your payment method and re-enable usage billing to keep calling."
        }
        other => other.message(),
    }
}
