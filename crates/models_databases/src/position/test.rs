use super::*;

#[test]
fn an_empty_list_starts_in_the_middle_of_the_key_space() {
    assert_eq!(key_between(None, None), Ok("80".to_string()));
}

#[test]
fn appending_and_prepending_step_one_byte_at_a_time() {
    assert_eq!(key_between(Some("80"), None), Ok("8180".to_string()));
    assert_eq!(key_between(Some("8180"), None), Ok("8280".to_string()));
    assert_eq!(key_between(Some("ff80"), None), Ok("ff8180".to_string()));
    assert_eq!(key_between(None, Some("80")), Ok("7f80".to_string()));
}

#[test]
fn a_key_between_neighbours_sorts_strictly_between_them() {
    assert_eq!(
        key_between(Some("80"), Some("8180")),
        Ok("817f80".to_string())
    );
    assert_eq!(
        key_between(Some("8180"), Some("8280")),
        Ok("818180".to_string())
    );

    let mut lower = "80".to_string();
    let upper = "8180".to_string();
    for _ in 0..500 {
        let key = key_between(Some(&lower), Some(&upper)).unwrap();
        assert!(lower < key && key < upper, "{lower} < {key} < {upper}");
        lower = key;
    }
}

#[test]
fn bounds_that_are_not_in_order_are_refused() {
    assert_eq!(
        key_between(Some("8180"), Some("8180")),
        Err(PositionError::OutOfOrder {
            before: "8180".into(),
            after: "8180".into(),
        })
    );
    assert_eq!(
        key_between(Some("8280"), Some("8180")),
        Err(PositionError::OutOfOrder {
            before: "8280".into(),
            after: "8180".into(),
        })
    );
}

#[test]
fn a_string_that_is_not_a_key_is_refused() {
    for written in ["", "zz", "81", "000000000001"] {
        assert_eq!(
            key_between(Some(written), None),
            Err(PositionError::NotAKey(written.into())),
            "{written:?}"
        );
    }
}

#[test]
fn many_keys_at_once_bisect_their_range() {
    assert_eq!(
        keys_between(None, None, 5),
        Ok(vec![
            "7e80".to_string(),
            "7f80".to_string(),
            "80".to_string(),
            "817f80".to_string(),
            "8180".to_string(),
        ])
    );
    assert_eq!(
        keys_between(Some("8180"), Some("8280"), 3),
        Ok(vec![
            "81817f80".to_string(),
            "818180".to_string(),
            "818280".to_string(),
        ])
    );
    assert_eq!(keys_between(Some("80"), None, 0), Ok(vec![]));

    let keys = keys_between(None, None, 100_000).unwrap();
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, keys);
    assert!(keys.iter().all(|key| key.len() <= 34));
}
