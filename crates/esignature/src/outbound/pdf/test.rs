use super::*;
use crate::domain::models::*;
use chrono::Utc;
use uuid::Uuid;
fn fixture() -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let resources = doc.add_object(dictionary! {"Font"=>dictionary!{"OriginalFont"=>font}});
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["OriginalFont".into(), 14.into()]),
            Operation::new("Td", vec![50.into(), 700.into()]),
            Operation::new(
                "Tj",
                vec![Object::string_literal("Original agreement text")],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page = doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),612.into(),792.into()],"Contents"=>stream});
    doc.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1,"Resources"=>resources}
            .into(),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", root);
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    bytes
}
fn envelope() -> Envelope {
    let r = Recipient {
        id: Uuid::now_v7(),
        name: "José Müller".into(),
        email: "jose@example.com".into(),
        order: 1,
        signed_at: Some(Utc::now()),
        delivered_at: Some(Utc::now()),
    };
    Envelope {
        id: Uuid::now_v7(),
        title: "Mutual NDA".into(),
        message: String::new(),
        filename: "NDA.pdf".into(),
        page_count: 1,
        source_sha256: "0123456789abcdef".repeat(4),
        completed_sha256: None,
        status: Status::Completed,
        revision: 4,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        expires_at: None,
        fields: vec![Field {
            id: Uuid::now_v7(),
            recipient_id: r.id,
            kind: FieldKind::Signature,
            page: 1,
            x: 0.1,
            y: 0.8,
            width: 0.3,
            height: 0.05,
            required: true,
            value: Some("José Müller".into()),
        }],
        recipients: vec![r],
        audit: vec![AuditEvent {
            at: Utc::now(),
            action: AuditAction::Signed,
            actor: "José Müller".into(),
            detail: "Consented to electronic records and signatures".into(),
        }],
    }
}
#[test]
fn preserves_original_fonts_unicode_and_appends_certificate() {
    let bytes = fixture();
    assert_eq!(Pdf.inspect(&bytes).unwrap(), 1);
    let completed = Pdf.complete(&bytes, &envelope()).unwrap();
    let doc = Document::load_mem(&completed).unwrap();
    assert_eq!(doc.get_pages().len(), 2);
    let text = doc.extract_text(&[1, 2]).unwrap();
    for expected in [
        "Original agreement text",
        "José Müller",
        "CERTIFICATE OF COMPLETION",
        "Consented to electronic records",
    ] {
        assert!(text.contains(expected), "Missing {expected}: {text}");
    }
    assert_eq!(Pdf.inspect(&bytes).unwrap(), 1);
}
#[test]
fn rejects_active_or_malformed_pdf() {
    assert!(Pdf.inspect(b"not a pdf").is_err());
    let mut doc = Document::load_mem(&fixture()).unwrap();
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(root).unwrap().set(
        "OpenAction",
        dictionary! {"S"=>"JavaScript","JS"=>Object::string_literal("alert('bad')")},
    );
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    assert!(Pdf.inspect(&bytes).is_err());
}
#[test]
fn validates_signature_unicode_before_mutation() {
    assert!(
        Pdf.validate_text("José Müller / Αλέξανδρος / Алексей")
            .is_ok()
    );
    assert!(Pdf.validate_text("🧑‍🚀").is_err());
}
