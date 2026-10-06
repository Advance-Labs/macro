//! Domain layer: plans and the configured pricing, the settlement ledger, ports,
//! and the billing service.

pub mod admission;
pub mod financial;
pub mod ledger;
pub mod models;
pub mod period;
pub mod policy;
pub mod ports;
pub mod pricing;
pub mod service;

pub use admission::{
    AdmissionFuture, AiAdmissionError, AiAdmissionService, BillingAdmissionService,
    DisabledAiAdmissionService,
};
pub use ledger::{
    SeatPhone, SettlementPlan, SettlementPolicy, SettlementState, decide_phone, plan_settlement,
};
pub use models::{
    AiUsageBilling, AllowanceDecision, AllowanceStore, BillingError, BillingPeriod,
    BillingSettings, CREDIT_PACKS_CENTS, DenyReason, Entitlement, MIN_STRIPE_CHARGE_CENTS,
    OVERAGE_CHARGE_THRESHOLD_CENTS, OVERAGE_LIMIT_MAX_CENTS, OVERAGE_LIMIT_MIN_CENTS,
    OpenPeriodStart, OverageChargeStatus, PHONE_ADDON_MONTHLY_PRICE_CENTS, PayerScope,
    PeriodAllowance, PeriodLedger, PhoneAddonOverview, PhoneAddonSeat, PhoneSeatStatus, PlanTier,
    Result, SeatAllowance, SeatGeneration, SeatUsage, SubscriptionScope, UsagePolicy,
    UsageSnapshot,
};
pub use ports::{
    BillingRepo, BillingService, CreditCheckoutRequest, EntitlementSource, OverageChargeRequest,
    PaymentGateway, PendingCharge, PhoneAddonQuantity, SettlementOutcome, SettlementTrigger,
    UsageReader,
};
pub use pricing::{
    AiPricing, IncludedAllowanceCents, IncludedPhoneMinutes, OverageMarkupPercent, PlanAllowances,
    PricingError, cost_cents,
};
pub use service::BillingServiceImpl;
