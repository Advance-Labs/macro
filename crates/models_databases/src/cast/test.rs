use super::*;

use Contents::{Empty, Filled};

fn select() -> CastKind {
    CastKind::Select { multi: false }
}

fn multi_select() -> CastKind {
    CastKind::Select { multi: true }
}

fn people() -> CastKind {
    CastKind::Entity {
        target: EntityKind::User,
        multi: false,
    }
}

fn several_people() -> CastKind {
    CastKind::Entity {
        target: EntityKind::User,
        multi: true,
    }
}

fn documents() -> CastKind {
    CastKind::Entity {
        target: EntityKind::Document,
        multi: false,
    }
}

fn tasks() -> CastKind {
    CastKind::Entity {
        target: EntityKind::Task,
        multi: false,
    }
}

fn relation() -> CastKind {
    CastKind::Relation
}

#[test]
fn text_is_checked_to_every_scalar_and_select_and_never_to_references() {
    let text = CastKind::Text;
    assert_eq!(cast(text, CastKind::Number, Filled), Cast::Checked);
    assert_eq!(cast(text, CastKind::Date, Filled), Cast::Checked);
    assert_eq!(cast(text, CastKind::Boolean, Filled), Cast::Checked);
    assert_eq!(cast(text, CastKind::Link, Filled), Cast::Checked);
    assert_eq!(cast(text, select(), Filled), Cast::Checked);
    assert_eq!(cast(text, multi_select(), Filled), Cast::Checked);
    assert_eq!(
        cast(text, people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
    assert_eq!(
        cast(text, documents(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
    assert_eq!(
        cast(text, tasks(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn number_is_safe_to_text_and_select_and_never_to_date_checkbox_or_url() {
    let number = CastKind::Number;
    assert_eq!(cast(number, CastKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(number, select(), Filled), Cast::Safe);
    assert_eq!(cast(number, multi_select(), Filled), Cast::Safe);
    assert_eq!(
        cast(number, CastKind::Date, Filled),
        Cast::Never("Numbers aren't dates.")
    );
    assert_eq!(
        cast(number, CastKind::Boolean, Filled),
        Cast::Never("Numbers aren't checkboxes.")
    );
    assert_eq!(
        cast(number, CastKind::Link, Filled),
        Cast::Never("Numbers aren't URLs.")
    );
    assert_eq!(
        cast(number, people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn checkbox_is_safe_to_text_and_never_to_anything_else() {
    let checkbox = CastKind::Boolean;
    let never = Cast::Never("A checkbox can only become text.");
    assert_eq!(cast(checkbox, CastKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(checkbox, CastKind::Number, Filled), never);
    assert_eq!(cast(checkbox, CastKind::Date, Filled), never);
    assert_eq!(cast(checkbox, CastKind::Link, Filled), never);
    assert_eq!(cast(checkbox, select(), Filled), never);
    assert_eq!(cast(checkbox, multi_select(), Filled), never);
    assert_eq!(
        cast(checkbox, tasks(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn date_is_safe_to_text_and_never_to_anything_else() {
    let date = CastKind::Date;
    let never = Cast::Never("A date can only become text.");
    assert_eq!(cast(date, CastKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(date, CastKind::Number, Filled), never);
    assert_eq!(cast(date, CastKind::Boolean, Filled), never);
    assert_eq!(cast(date, CastKind::Link, Filled), never);
    assert_eq!(cast(date, select(), Filled), never);
    assert_eq!(cast(date, multi_select(), Filled), never);
    assert_eq!(
        cast(date, documents(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn select_is_safe_to_text_and_multi_select_and_checked_to_number_url_date_and_checkbox() {
    assert_eq!(cast(select(), CastKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(select(), multi_select(), Filled), Cast::Safe);
    assert_eq!(cast(select(), CastKind::Number, Filled), Cast::Checked);
    assert_eq!(cast(select(), CastKind::Link, Filled), Cast::Checked);
    assert_eq!(cast(select(), CastKind::Date, Filled), Cast::Checked);
    assert_eq!(cast(select(), CastKind::Boolean, Filled), Cast::Checked);
    assert_eq!(
        cast(select(), people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn multi_select_is_checked_to_every_single_valued_type_and_never_to_references() {
    assert_eq!(cast(multi_select(), select(), Filled), Cast::Checked);
    assert_eq!(cast(multi_select(), CastKind::Text, Filled), Cast::Checked);
    assert_eq!(cast(multi_select(), CastKind::Link, Filled), Cast::Checked);
    assert_eq!(
        cast(multi_select(), CastKind::Number, Filled),
        Cast::Checked
    );
    assert_eq!(cast(multi_select(), CastKind::Date, Filled), Cast::Checked);
    assert_eq!(
        cast(multi_select(), CastKind::Boolean, Filled),
        Cast::Checked
    );
    assert_eq!(
        cast(multi_select(), several_people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn url_is_safe_to_text_checked_to_select_and_never_to_the_rest() {
    let url = CastKind::Link;
    let never = Cast::Never("A URL can only become text or a select.");
    assert_eq!(cast(url, CastKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(url, select(), Filled), Cast::Checked);
    assert_eq!(cast(url, multi_select(), Filled), never);
    assert_eq!(cast(url, CastKind::Number, Filled), never);
    assert_eq!(cast(url, CastKind::Date, Filled), never);
    assert_eq!(cast(url, CastKind::Boolean, Filled), never);
    assert_eq!(
        cast(url, people(), Filled),
        Cast::Never("Only an empty column can become a reference column.")
    );
}

#[test]
fn references_widen_safely_narrow_checked_and_never_change_kind_or_become_values() {
    assert_eq!(cast(people(), several_people(), Filled), Cast::Safe);
    assert_eq!(cast(several_people(), people(), Filled), Cast::Checked);
    assert_eq!(
        cast(people(), documents(), Filled),
        Cast::Never("References can't change what they point at.")
    );
    assert_eq!(
        cast(tasks(), documents(), Filled),
        Cast::Never("References can't change what they point at.")
    );
    assert_eq!(
        cast(people(), CastKind::Text, Filled),
        Cast::Never("References can't become plain values.")
    );
    assert_eq!(
        cast(documents(), select(), Filled),
        Cast::Never("References can't become plain values.")
    );
}

#[test]
fn relations_are_never_made_or_converted_while_they_hold_values() {
    let to_relation =
        Cast::Never("Only an empty column can become a relation: existing values aren't rows.");
    assert_eq!(cast(CastKind::Text, relation(), Filled), to_relation);
    assert_eq!(cast(several_people(), relation(), Filled), to_relation);
    assert_eq!(
        cast(relation(), CastKind::Text, Filled),
        Cast::Never("A relation's linked rows can't be converted; remove them first.")
    );
}

#[test]
fn an_empty_column_takes_any_type() {
    assert_eq!(cast(CastKind::Text, people(), Empty), Cast::Safe);
    assert_eq!(cast(CastKind::Date, CastKind::Number, Empty), Cast::Safe);
    assert_eq!(cast(people(), documents(), Empty), Cast::Safe);
    assert_eq!(cast(CastKind::Number, relation(), Empty), Cast::Safe);
    assert_eq!(cast(relation(), select(), Empty), Cast::Safe);
}

#[test]
fn a_type_to_itself_is_safe() {
    assert_eq!(cast(CastKind::Text, CastKind::Text, Filled), Cast::Safe);
    assert_eq!(cast(CastKind::Number, CastKind::Number, Filled), Cast::Safe);
    assert_eq!(
        cast(CastKind::Boolean, CastKind::Boolean, Filled),
        Cast::Safe
    );
    assert_eq!(cast(CastKind::Date, CastKind::Date, Filled), Cast::Safe);
    assert_eq!(cast(CastKind::Link, CastKind::Link, Filled), Cast::Safe);
    assert_eq!(cast(select(), select(), Filled), Cast::Safe);
    assert_eq!(cast(multi_select(), multi_select(), Filled), Cast::Safe);
    assert_eq!(cast(people(), people(), Filled), Cast::Safe);
}

#[test]
fn column_kinds_read_as_their_type_names() {
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
        ColumnKind::SelectNumber { multi: true }.to_string(),
        "select_number[]"
    );
    assert_eq!(ColumnKind::Tag.to_string(), "tag");
    assert_eq!(
        ColumnKind::Entity {
            target: EntityKind::CalendarEvent,
            multi: true
        }
        .to_string(),
        "entity(CALENDAR_EVENT)[]"
    );
}

#[test]
fn numeric_selects_and_tags_cast_as_selects_and_a_relation_as_rows() {
    assert_eq!(
        CastKind::from(ColumnKind::SelectNumber { multi: false }),
        CastKind::Select { multi: false }
    );
    assert_eq!(
        CastKind::from(ColumnKind::Tag),
        CastKind::Select { multi: true }
    );
    assert_eq!(
        CastKind::from(ColumnKind::Relation {
            database: uuid::Uuid::nil(),
            table: uuid::Uuid::nil(),
        }),
        CastKind::Relation
    );
}

#[test]
fn whole_numbers_label_without_a_fraction() {
    assert_eq!(number_label(2.0), "2");
    assert_eq!(number_label(2.5), "2.5");
}
