use super::*;

use std::sync::Mutex;
use std::time::Duration;

use ai_billing::AdmissionFuture;
use ai_usage::{UsageAmount, UsageEvent};
use uuid::Uuid;

struct FakeAdmission(Result<(), AiAdmissionError>);

impl AiAdmissionService for FakeAdmission {
    fn admit<'a>(
        &'a self,
        _user: &'a MacroUserIdStr<'_>,
        feature: AiFeature,
    ) -> AdmissionFuture<'a> {
        assert_eq!(feature, AiFeature::PhoneCall);
        let result = self.0;
        Box::pin(async move { result })
    }
}

#[derive(Default)]
struct FakeRecorder(Mutex<Vec<UsageEvent>>);

impl UsageRecorder for FakeRecorder {
    fn record(&self, event: UsageEvent) {
        self.0.lock().unwrap().push(event);
    }
}

fn user() -> MacroUserIdStr<'static> {
    MacroUserIdStr::try_from("macro|caller@example.com".to_string()).unwrap()
}

fn billing(admission: Result<(), AiAdmissionError>) -> (AiPhoneBilling, Arc<FakeRecorder>) {
    let recorder = Arc::new(FakeRecorder::default());
    (
        AiPhoneBilling::new(Arc::new(FakeAdmission(admission)), recorder.clone()),
        recorder,
    )
}

#[tokio::test]
async fn admission_maps_to_phone_billing() {
    let (allowed, _) = billing(Ok(()));
    assert_eq!(allowed.admit(&user()).await, Ok(()));

    let (unavailable, _) = billing(Err(AiAdmissionError::Unavailable));
    assert_eq!(
        unavailable.admit(&user()).await,
        Err(PhoneBillingError::Unavailable)
    );

    let (plan, _) = billing(Err(AiAdmissionError::Denied(DenyReason::PhonePlanRequired)));
    assert_eq!(
        plan.admit(&user()).await,
        Err(PhoneBillingError::Denied {
            code: "phone_plan_required",
            message: DenyReason::PhonePlanRequired.message(),
        })
    );

    // Shared overage reasons keep their code but speak of calling.
    let (limit, _) = billing(Err(AiAdmissionError::Denied(
        DenyReason::OverageLimitReached,
    )));
    let Err(PhoneBillingError::Denied { code, message }) = limit.admit(&user()).await else {
        panic!("expected a denial");
    };
    assert_eq!(code, "ai_overage_limit_reached");
    assert!(message.contains("keep calling"), "{message}");
}

#[test]
fn minutes_are_recorded_as_phone_usage_on_the_call() {
    let (billing, recorder) = billing(Ok(()));
    let call_id = Uuid::now_v7();
    billing.record(PhoneUsage {
        user: user(),
        call_id,
        billed: Duration::from_secs(180),
    });
    let events = recorder.0.lock().unwrap();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.feature, AiFeature::PhoneCall);
    assert_eq!(event.user.as_ref(), user().as_ref());
    assert_eq!(event.entity, Some(call_id));
    assert_eq!(event.model, PHONE_USAGE_MODEL);
    assert!(matches!(
        event.amount,
        UsageAmount::Audio { duration } if duration == Duration::from_secs(180)
    ));
}
