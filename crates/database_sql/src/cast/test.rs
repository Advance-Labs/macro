use super::*;
use crate::catalog::SelectOption;
use uuid::Uuid;

use Contents::{Empty, Filled};

fn select() -> ColumnKind {
    ColumnKind::Select {
        multi: false,
        options: vec![SelectOption {
            id: Uuid::from_u128(1),
            label: "Won".into(),
        }],
    }
}

fn multi_select() -> ColumnKind {
    ColumnKind::Select {
        multi: true,
        options: vec![],
    }
}

fn people() -> ColumnKind {
    ColumnKind::Entity {
        multi: false,
        target: EntityKind::User,
    }
}

fn several_people() -> ColumnKind {
    ColumnKind::Entity {
        multi: true,
        target: EntityKind::User,
    }
}

fn documents() -> ColumnKind {
    ColumnKind::Entity {
        multi: false,
        target: EntityKind::Document,
    }
}

fn tasks() -> ColumnKind {
    ColumnKind::Entity {
        multi: false,
        target: EntityKind::Task,
    }
}

fn relation() -> ColumnKind {
    ColumnKind::Entity {
        multi: true,
        target: EntityKind::Row,
    }
}

#[test]
fn text_is_checked_to_every_scalar_and_select_and_never_to_references() {
    let text = ColumnKind::Text;
    assert_eq!(cast(&text, &ColumnKind::Number, Filled), Cast::Checked);
    assert_eq!(cast(&text, &ColumnKind::Date, Filled), Cast::Checked);
    assert_eq!(cast(&text, &ColumnKind::Boolean, Filled), Cast::Checked);
    assert_eq!(cast(&text, &ColumnKind::Link, Filled), Cast::Checked);
    assert_eq!(cast(&text, &select(), Filled), Cast::Checked);
    assert_eq!(cast(&text, &multi_select(), Filled), Cast::Checked);
    assert_eq!(
        cast(&text, &people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
    assert_eq!(
        cast(&text, &documents(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
    assert_eq!(
        cast(&text, &tasks(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn number_is_safe_to_text_and_select_and_never_to_date_checkbox_or_url() {
    let number = ColumnKind::Number;
    assert_eq!(cast(&number, &ColumnKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(&number, &select(), Filled), Cast::Safe);
    assert_eq!(cast(&number, &multi_select(), Filled), Cast::Safe);
    assert_eq!(
        cast(&number, &ColumnKind::Date, Filled),
        Cast::Never("Numbers aren't dates.")
    );
    assert_eq!(
        cast(&number, &ColumnKind::Boolean, Filled),
        Cast::Never("Numbers aren't checkboxes.")
    );
    assert_eq!(
        cast(&number, &ColumnKind::Link, Filled),
        Cast::Never("Numbers aren't URLs.")
    );
    assert_eq!(
        cast(&number, &people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn checkbox_is_safe_to_text_and_never_to_anything_else() {
    let checkbox = ColumnKind::Boolean;
    let never = Cast::Never("A checkbox can only become text.");
    assert_eq!(cast(&checkbox, &ColumnKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(&checkbox, &ColumnKind::Number, Filled), never);
    assert_eq!(cast(&checkbox, &ColumnKind::Date, Filled), never);
    assert_eq!(cast(&checkbox, &ColumnKind::Link, Filled), never);
    assert_eq!(cast(&checkbox, &select(), Filled), never);
    assert_eq!(cast(&checkbox, &multi_select(), Filled), never);
    assert_eq!(
        cast(&checkbox, &tasks(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn date_is_safe_to_text_and_never_to_anything_else() {
    let date = ColumnKind::Date;
    let never = Cast::Never("A date can only become text.");
    assert_eq!(cast(&date, &ColumnKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(&date, &ColumnKind::Number, Filled), never);
    assert_eq!(cast(&date, &ColumnKind::Boolean, Filled), never);
    assert_eq!(cast(&date, &ColumnKind::Link, Filled), never);
    assert_eq!(cast(&date, &select(), Filled), never);
    assert_eq!(cast(&date, &multi_select(), Filled), never);
    assert_eq!(
        cast(&date, &documents(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn select_is_safe_to_text_and_multi_select_and_checked_to_number_url_date_and_checkbox() {
    assert_eq!(cast(&select(), &ColumnKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(&select(), &multi_select(), Filled), Cast::Safe);
    assert_eq!(cast(&select(), &ColumnKind::Number, Filled), Cast::Checked);
    assert_eq!(cast(&select(), &ColumnKind::Link, Filled), Cast::Checked);
    assert_eq!(cast(&select(), &ColumnKind::Date, Filled), Cast::Checked);
    assert_eq!(cast(&select(), &ColumnKind::Boolean, Filled), Cast::Checked);
    assert_eq!(
        cast(&select(), &people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn multi_select_is_checked_to_every_single_valued_type_and_never_to_references() {
    assert_eq!(cast(&multi_select(), &select(), Filled), Cast::Checked);
    assert_eq!(
        cast(&multi_select(), &ColumnKind::Text, Filled),
        Cast::Checked
    );
    assert_eq!(
        cast(&multi_select(), &ColumnKind::Link, Filled),
        Cast::Checked
    );
    assert_eq!(
        cast(&multi_select(), &ColumnKind::Number, Filled),
        Cast::Checked
    );
    assert_eq!(
        cast(&multi_select(), &ColumnKind::Date, Filled),
        Cast::Checked
    );
    assert_eq!(
        cast(&multi_select(), &ColumnKind::Boolean, Filled),
        Cast::Checked
    );
    assert_eq!(
        cast(&multi_select(), &several_people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn url_is_safe_to_text_checked_to_select_and_never_to_the_rest() {
    let url = ColumnKind::Link;
    let never = Cast::Never("A URL can only become text or a select.");
    assert_eq!(cast(&url, &ColumnKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(&url, &select(), Filled), Cast::Checked);
    assert_eq!(cast(&url, &multi_select(), Filled), never);
    assert_eq!(cast(&url, &ColumnKind::Number, Filled), never);
    assert_eq!(cast(&url, &ColumnKind::Date, Filled), never);
    assert_eq!(cast(&url, &ColumnKind::Boolean, Filled), never);
    assert_eq!(
        cast(&url, &people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn references_widen_safely_narrow_checked_and_never_change_kind_or_become_values() {
    assert_eq!(cast(&people(), &several_people(), Filled), Cast::Safe);
    assert_eq!(cast(&several_people(), &people(), Filled), Cast::Checked);
    assert_eq!(
        cast(&people(), &documents(), Filled),
        Cast::Never("References can't change what they point at.")
    );
    assert_eq!(
        cast(&tasks(), &documents(), Filled),
        Cast::Never("References can't change what they point at.")
    );
    assert_eq!(
        cast(&people(), &ColumnKind::Text, Filled),
        Cast::Never("References can't become plain values.")
    );
    assert_eq!(
        cast(&documents(), &select(), Filled),
        Cast::Never("References can't become plain values.")
    );
}

#[test]
fn relations_are_never_made_or_converted_while_they_hold_values() {
    let to_relation =
        Cast::Never("Only an empty column can become a relation: existing values aren't rows.");
    assert_eq!(cast(&ColumnKind::Text, &relation(), Filled), to_relation);
    assert_eq!(cast(&several_people(), &relation(), Filled), to_relation);
    assert_eq!(
        cast(&relation(), &ColumnKind::Text, Filled),
        Cast::Never("A relation's linked rows can't be converted; remove them first.")
    );
}

#[test]
fn an_empty_column_takes_any_type() {
    assert_eq!(cast(&ColumnKind::Text, &people(), Empty), Cast::Safe);
    assert_eq!(
        cast(&ColumnKind::Date, &ColumnKind::Number, Empty),
        Cast::Safe
    );
    assert_eq!(cast(&people(), &documents(), Empty), Cast::Safe);
    assert_eq!(cast(&ColumnKind::Number, &relation(), Empty), Cast::Safe);
    assert_eq!(cast(&relation(), &select(), Empty), Cast::Safe);
}

#[test]
fn a_type_to_itself_is_safe() {
    assert_eq!(
        cast(&ColumnKind::Text, &ColumnKind::Text, Filled),
        Cast::Safe
    );
    assert_eq!(
        cast(&ColumnKind::Number, &ColumnKind::Number, Filled),
        Cast::Safe
    );
    assert_eq!(
        cast(&ColumnKind::Boolean, &ColumnKind::Boolean, Filled),
        Cast::Safe
    );
    assert_eq!(
        cast(&ColumnKind::Date, &ColumnKind::Date, Filled),
        Cast::Safe
    );
    assert_eq!(
        cast(&ColumnKind::Link, &ColumnKind::Link, Filled),
        Cast::Safe
    );
    assert_eq!(cast(&select(), &select(), Filled), Cast::Safe);
    assert_eq!(cast(&multi_select(), &multi_select(), Filled), Cast::Safe);
    assert_eq!(cast(&people(), &people(), Filled), Cast::Safe);
}

#[test]
fn column_types_read_as_sql_names() {
    let names: Vec<String> = TARGETS.iter().map(ToString::to_string).collect();
    assert_eq!(
        names,
        [
            "text",
            "number",
            "select",
            "select[]",
            "date",
            "boolean",
            "link",
            "entity(USER)",
            "entity(DOCUMENT)",
            "entity(TASK)",
        ]
    );
    assert_eq!(
        ColumnType::SelectNumber { multi: true }.to_string(),
        "select_number[]"
    );
    assert_eq!(ColumnType::Tag.to_string(), "tag");
    assert_eq!(
        ColumnType::Entity {
            target: EntityKind::CalendarEvent,
            multi: true
        }
        .to_string(),
        "entity(CALENDAR_EVENT)[]"
    );
}

#[test]
fn a_column_type_has_the_kind_of_a_column_of_that_type() {
    assert_eq!(
        ColumnType::SelectNumber { multi: false }.kind(),
        ColumnKind::Select {
            multi: false,
            options: vec![]
        }
    );
    assert_eq!(
        ColumnType::Tag.kind(),
        ColumnKind::Select {
            multi: true,
            options: vec![]
        }
    );
    assert_eq!(
        ColumnType::Entity {
            target: EntityKind::Task,
            multi: true
        }
        .kind(),
        ColumnKind::Entity {
            multi: true,
            target: EntityKind::Task
        }
    );
}
