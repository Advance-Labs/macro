use super::*;

fn client() -> fusionauth::FusionAuthClient {
    fusionauth::FusionAuthClient::new(
        "api-key".to_owned(),
        "client-id".to_owned(),
        "client-secret".to_owned(),
        "http://fusionauth.invalid".to_owned(),
        "http://localhost/mcp/oauth/callback".to_owned(),
        "google-client-id".to_owned(),
        "google-client-secret".to_owned(),
    )
}

#[test]
fn authorize_url_fails_without_google_idp() {
    let provider = FusionAuthOAuthProvider::with_google_idp_id(client(), None);
    let err = provider
        .construct_authorize_url("state")
        .expect_err("authorize must fail without a Google IdP");
    assert!(err.to_string().contains("not configured"), "{err}");
}

#[test]
fn authorize_url_uses_google_idp_when_present() {
    let provider =
        FusionAuthOAuthProvider::with_google_idp_id(client(), Some("idp-123".to_owned()));
    let url = provider
        .construct_authorize_url("state")
        .expect("authorize URL with a Google IdP");
    assert!(url.contains("idp-123"), "{url}");
}
