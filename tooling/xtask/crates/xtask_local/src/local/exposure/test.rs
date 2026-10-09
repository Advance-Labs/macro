use super::*;

#[test]
fn loopback_port_binds_every_short_form_to_localhost() {
    assert_eq!(loopback_port("31000:5432"), "127.0.0.1:31000:5432");
    assert_eq!(loopback_port("8080"), "127.0.0.1::8080");
    assert_eq!(loopback_port("127.0.0.1:31023:22"), "127.0.0.1:31023:22");
    assert_eq!(loopback_port("0.0.0.0:80:80"), "0.0.0.0:80:80");
}

#[test]
fn truthy_values() {
    for v in ["1", "true", "TRUE", " yes ", "on"] {
        assert!(is_truthy(v), "{v}");
    }
    for v in ["", "0", "false", "off", "no"] {
        assert!(!is_truthy(v), "{v}");
    }
}
