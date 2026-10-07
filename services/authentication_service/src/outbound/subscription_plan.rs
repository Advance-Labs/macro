//! Authenticated customer lookup and Stripe subscription translation.
use crate::service::subscription_plan::{
    PersonalSubscription, PlanAction, PlanChangeError, PlanGateway,
};
use macro_user_id::user_id::MacroUserIdStr;
use sqlx::PgPool;
use std::sync::Arc;
use teams::domain::{
    customer_repo::CustomerRepository,
    model::{SeatPlan, SeatPrices},
};
/// Composed with the owning billing adapter through its domain port.
pub struct StripePlanGateway<C> {
    db: PgPool,
    stripe: Arc<stripe::Client>,
    prices: SeatPrices,
    customer: C,
}
impl<C> StripePlanGateway<C> {
    /// Inject customer storage, provider client and billing domain capabilities.
    pub fn new(db: PgPool, stripe: Arc<stripe::Client>, prices: SeatPrices, customer: C) -> Self {
        Self {
            db,
            stripe,
            prices,
            customer,
        }
    }
}
impl<C: CustomerRepository> PlanGateway for StripePlanGateway<C> {
    async fn personal_subscription(
        &self,
        user: &MacroUserIdStr<'_>,
    ) -> Result<PersonalSubscription, PlanChangeError> {
        let id = macro_db_client::user::get::get_stripe_customer_id_by_user_id(&self.db, user)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or(PlanChangeError::MissingCustomer)?;
        let mut params = stripe::ListSubscriptions::new();
        params.customer = Some(id.parse().map_err(anyhow::Error::from)?);
        params.limit = Some(100);
        let mut subscriptions = Vec::new();
        loop {
            let page = stripe::Subscription::list(&self.stripe, &params)
                .await
                .map_err(anyhow::Error::from)?;
            let last = page.data.last().map(|s| s.id.clone());
            subscriptions.extend(page.data.into_iter().filter(|s| {
                !s.metadata.contains_key("team_id")
                    && matches!(
                        s.status,
                        stripe::SubscriptionStatus::Active | stripe::SubscriptionStatus::Trialing
                    )
            }));
            if !page.has_more {
                break;
            }
            params.starting_after =
                Some(last.ok_or_else(|| anyhow::anyhow!("missing pagination cursor"))?);
        }
        if subscriptions.len() > 1 {
            return Err(PlanChangeError::AmbiguousSubscription);
        }
        let subscription = subscriptions.pop().ok_or(PlanChangeError::NoSubscription)?;
        let seats = subscription
            .items
            .data
            .iter()
            .filter_map(|item| {
                let plan = item
                    .price
                    .as_ref()
                    .and_then(|p| self.prices.plan_for_price(p.id.as_str()))?;
                Some((plan, item.quantity.unwrap_or(1)))
            })
            .collect::<Vec<_>>();
        let plan = match seats.as_slice() {
            [(plan, 1)] => *plan,
            [] => return Err(PlanChangeError::NoSubscription),
            _ => return Err(PlanChangeError::AmbiguousSubscription),
        };
        Ok(PersonalSubscription {
            id: subscription.id.to_string(),
            plan,
        })
    }
    async fn apply(
        &self,
        user: &MacroUserIdStr<'_>,
        subscription: PersonalSubscription,
        target: SeatPlan,
        action: PlanAction,
    ) -> Result<(), PlanChangeError> {
        self.prices
            .price_id(target)
            .map_err(|_| PlanChangeError::PlanUnavailable)?;
        let id = subscription.id.parse().map_err(anyhow::Error::from)?;
        match action {
            PlanAction::ScheduleDowngrade => {
                self.customer
                    .schedule_seat_plan(&id, user, Some(target))
                    .await
            }
            PlanAction::KeepCurrent => self.customer.schedule_seat_plan(&id, user, None).await,
            PlanAction::Upgrade => {
                self.customer
                    .schedule_seat_plan(&id, user, None)
                    .await
                    .map_err(anyhow::Error::from)?;
                self.customer.upgrade_personal_plan(&id, target).await
            }
        }
        .map_err(anyhow::Error::from)?;
        Ok(())
    }
}
