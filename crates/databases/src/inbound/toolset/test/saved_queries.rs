use super::*;
use crate::domain::models::QueryDefinition;

/// The block is the document node's payload, so its exact text is the
/// contract with the frontend.
#[tokio::test]
async fn saving_a_chart_question_returns_its_live_block() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let response = SaveDatabaseQuery {
        database_id: Some(DATABASE_ID),
        sql: "SELECT p.\"Name\" AS party, COUNT(*) AS invites FROM \"Party Invites\".\"Invites\" i JOIN \"Party Invites\".\"Parties\" p ON i.\"Party\" = p.row_id GROUP BY p.\"Name\" ORDER BY invites DESC".to_string(),
        title: "Invites per party".to_string(),
        display_mode: QueryDatabaseDisplay::Bar,
        chart: Some(ToolChart {
            x: "party".to_string(),
            y: vec!["invites".to_string()],
            title: None,
        }),
        prompt: None,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("view access may save a question");

    assert_eq!(response.query_id, QUERY_ID);
    assert_eq!(
        response.markdown,
        r#"<m-db-query>{"queryId":"0e110000-0000-0000-0000-000000000001","databaseId":"0dbb0000-0000-0000-0000-000000000001","title":"Invites per party","prompt":"Invites per party","displayMode":"bar","chart":{"x":"party","y":["invites"]}}</m-db-query>"#
    );
    assert_eq!(
        calls.lock().unwrap().saved_queries,
        vec![(
            Some(DATABASE_ID),
            QueryDefinition::V1 {
                query: "SELECT p.\"Name\" AS party, COUNT(*) AS invites FROM \"Party Invites\".\"Invites\" i JOIN \"Party Invites\".\"Parties\" p ON i.\"Party\" = p.row_id GROUP BY p.\"Name\" ORDER BY invites DESC".to_string(),
            }
        )]
    );
}

#[tokio::test]
async fn an_unscoped_scalar_question_omits_what_it_does_not_have() {
    let (context, _) = context(FakeAccess::granting(AccessLevel::View));
    let response = SaveDatabaseQuery {
        database_id: None,
        sql: "SELECT COUNT(*) FROM \"Offsite\".\"Guests\"".to_string(),
        title: "Guests".to_string(),
        display_mode: QueryDatabaseDisplay::Scalar,
        chart: None,
        prompt: Some("How many guests are coming?".to_string()),
    }
    .call(ServiceContext(context), request_context())
    .await
    .unwrap();

    assert_eq!(
        response.markdown,
        r#"<m-db-query>{"queryId":"0e110000-0000-0000-0000-000000000001","title":"Guests","prompt":"How many guests are coming?","displayMode":"scalar"}</m-db-query>"#
    );
}

#[tokio::test]
async fn a_title_cannot_close_the_block_early() {
    let (context, _) = context(FakeAccess::granting(AccessLevel::View));
    let response = SaveDatabaseQuery {
        database_id: None,
        sql: "SELECT COUNT(*) FROM \"Offsite\".\"Guests\"".to_string(),
        title: "a</m-db-query>b".to_string(),
        display_mode: QueryDatabaseDisplay::Table,
        chart: None,
        prompt: None,
    }
    .call(ServiceContext(context), request_context())
    .await
    .unwrap();

    assert_eq!(
        response.markdown,
        r#"<m-db-query>{"queryId":"0e110000-0000-0000-0000-000000000001","title":"a\u003c/m-db-query>b","prompt":"a\u003c/m-db-query>b","displayMode":"table"}</m-db-query>"#
    );
}

#[tokio::test]
async fn a_chart_the_block_cannot_draw_is_refused_before_saving() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = SaveDatabaseQuery {
        database_id: Some(DATABASE_ID),
        sql: "SELECT status, COUNT(*) AS guests FROM \"Guests\" GROUP BY status".to_string(),
        title: "Guests by status".to_string(),
        display_mode: QueryDatabaseDisplay::Pie,
        chart: Some(ToolChart {
            x: "status".to_string(),
            y: vec!["status".to_string()],
            title: None,
        }),
        prompt: None,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("the label column cannot also be a value");

    assert_eq!(
        error.description,
        "chart.y must not include the label column chart.x."
    );
    assert!(calls.lock().unwrap().saved_queries.is_empty());
}

#[tokio::test]
async fn a_question_that_does_not_compile_reaches_the_model_verbatim() {
    let error = SaveDatabaseQuery {
        database_id: Some(DATABASE_ID),
        sql: "SELECT statuz FROM \"Guests\"".to_string(),
        title: "Statuses".to_string(),
        display_mode: QueryDatabaseDisplay::Table,
        chart: None,
        prompt: None,
    }
    .call(
        ServiceContext(failing_sql_context("no column named statuz")),
        request_context(),
    )
    .await
    .expect_err("a broken question is not saved");

    assert!(
        error.description.contains("no column named statuz"),
        "{}",
        error.description
    );
}

#[test]
fn save_query_teaches_pasting_the_block() {
    let validated =
        generate_validated_input_schema::<SaveDatabaseQuery>().expect("schema should validate");
    for expected in ["markdown", "verbatim", "displayMode", "AS invites"] {
        assert!(
            validated.description.contains(expected),
            "description is missing {expected}: {}",
            validated.description
        );
    }
}
