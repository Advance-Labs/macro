use databases::domain::models::QueryDefinition;

use super::*;

const PARTY_SQL: &str =
    "SELECT \"Status\" AS status, COUNT(*) AS guests FROM \"Guests\" GROUP BY \"Status\"";

fn chart(x: &str, y: &[&str]) -> ToolChart {
    ToolChart {
        x: x.to_string(),
        y: y.iter().map(|name| name.to_string()).collect(),
        title: None,
        color: None,
        stack: None,
    }
}

fn question(display_mode: QueryDatabaseDisplay, chart: Option<ToolChart>) -> SaveDatabaseQuery {
    SaveDatabaseQuery {
        database_id: Some(OFFSITE),
        sql: PARTY_SQL.to_string(),
        title: "Guests by status".to_string(),
        display_mode,
        chart,
        prompt: None,
    }
}

/// The block is the document node's payload, so its exact text is the
/// contract with the frontend.
#[tokio::test]
async fn saving_a_chart_question_returns_its_live_block() {
    let world = world();
    let response = question(
        QueryDatabaseDisplay::Area,
        Some(ToolChart {
            color: Some("status".to_string()),
            stack: Some(true),
            ..chart("party", &["guests"])
        }),
    )
    .call(ServiceContext(context(&world)), as_user(VIEWER))
    .await
    .expect("view access may save a question");

    assert_eq!(
        response.markdown,
        r#"<m-db-query>{"queryId":"00000000-0000-0000-0000-000000000e11","databaseId":"00000000-0000-0000-0000-00000000db01","title":"Guests by status","prompt":"Guests by status","displayMode":"area","chart":{"x":"party","y":["guests"],"color":"status","stack":true}}</m-db-query>"#
    );
    assert_eq!(
        world.lock().unwrap().saved,
        vec![(
            Some(OFFSITE),
            QueryDefinition::V1 {
                query: PARTY_SQL.to_string(),
            }
        )]
    );
}

#[tokio::test]
async fn an_unstacked_chart_writes_no_stack() {
    let world = world();
    let response = question(
        QueryDatabaseDisplay::Scatter,
        Some(ToolChart {
            stack: Some(false),
            ..chart("status", &["guests"])
        }),
    )
    .call(ServiceContext(context(&world)), as_user(VIEWER))
    .await
    .unwrap();

    assert!(
        response
            .markdown
            .contains(r#""displayMode":"scatter","chart":{"x":"status","y":["guests"]}}"#),
        "{}",
        response.markdown
    );
}

#[tokio::test]
async fn an_unscoped_scalar_question_omits_what_it_does_not_have() {
    let world = world();
    let response = SaveDatabaseQuery {
        database_id: None,
        sql: "SELECT COUNT(*) FROM \"Offsite\".\"Guests\"".to_string(),
        title: "Guests".to_string(),
        display_mode: QueryDatabaseDisplay::Scalar,
        chart: None,
        prompt: Some("How many guests are coming?".to_string()),
    }
    .call(ServiceContext(context(&world)), as_user(VIEWER))
    .await
    .unwrap();

    assert_eq!(
        response.markdown,
        r#"<m-db-query>{"queryId":"00000000-0000-0000-0000-000000000e11","title":"Guests","prompt":"How many guests are coming?","displayMode":"scalar"}</m-db-query>"#
    );
}

#[tokio::test]
async fn a_title_cannot_close_the_block_early() {
    let world = world();
    let response = SaveDatabaseQuery {
        title: "a</m-db-query>b".to_string(),
        ..question(QueryDatabaseDisplay::Table, None)
    }
    .call(ServiceContext(context(&world)), as_user(VIEWER))
    .await
    .unwrap();

    assert!(
        response
            .markdown
            .contains(r#""title":"a\u003c/m-db-query>b","prompt":"a\u003c/m-db-query>b""#),
        "{}",
        response.markdown
    );
}

#[tokio::test]
async fn a_chart_the_block_cannot_draw_is_refused_before_saving() {
    let color = |color: &str, y: &[&str]| ToolChart {
        color: Some(color.to_string()),
        ..chart("status", y)
    };
    for (chart, refusal) in [
        (
            chart("status", &["status"]),
            "chart.y must not include the label column chart.x.",
        ),
        (
            color("status", &["guests"]),
            "chart.color must not be the label column chart.x.",
        ),
        (
            color("guests", &["guests"]),
            "chart.color must not be one of the chart.y columns.",
        ),
        (
            color("party", &["guests", "maybes"]),
            "chart.color splits a single series; with chart.color, chart.y names one column.",
        ),
        (
            color(" ", &["guests"]),
            "chart.color must name a result column.",
        ),
    ] {
        let world = world();
        let error = question(QueryDatabaseDisplay::Bar, Some(chart))
            .call(ServiceContext(context(&world)), as_user(VIEWER))
            .await
            .expect_err("the block cannot draw it");
        assert_eq!(error.description, refusal);
        assert!(world.lock().unwrap().saved.is_empty());
    }
}

#[tokio::test]
async fn a_question_that_does_not_compile_reaches_the_model_verbatim() {
    let world = world();
    let error = SaveDatabaseQuery {
        sql: "SELECT statuz FROM \"Guests\"".to_string(),
        ..question(QueryDatabaseDisplay::Table, None)
    }
    .call(ServiceContext(context(&world)), as_user(VIEWER))
    .await
    .expect_err("a broken question is not saved");

    assert!(
        error.description.contains("statuz"),
        "{}",
        error.description
    );
    assert!(world.lock().unwrap().saved.is_empty());
}

#[test]
fn save_query_teaches_pasting_the_block_and_every_chart() {
    let validated =
        generate_validated_input_schema::<SaveDatabaseQuery>().expect("schema should validate");
    for expected in [
        "markdown",
        "verbatim",
        "displayMode",
        "AS invites",
        "`area`",
        "`scatter`",
        "`color`",
        "`stack`",
    ] {
        assert!(
            validated.description.contains(expected),
            "description is missing {expected}: {}",
            validated.description
        );
    }
}
