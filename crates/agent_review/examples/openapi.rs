//! Export the review contract used to generate the browser's wire types.
fn main() {
    println!("{}", agent_review::inbound::axum_router::openapi());
}
