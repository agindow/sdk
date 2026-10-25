use agw_http_host::Envelope;
#[test]
fn preserves_raw_payload_tokens_and_requires_write_keys() {
    let json = r#"{"tool":"vendor.call","connection":"conn","operation":"write","args":{ "n":1e400,"n":2 },"mode":"repair","idempotency_key":"k"}"#;
    let envelope = serde_json::from_str::<Envelope>(json);
    assert!(envelope.is_ok_and(
        |input| input.validate().is_ok() && input.args.get() == r#"{ "n":1e400,"n":2 }"#
    ));
    let no_key = json.replace(",\"idempotency_key\":\"k\"", "");
    assert!(serde_json::from_str::<Envelope>(&no_key).is_ok_and(|input| input.validate().is_err()));
}
#[test]
fn rejects_inline_credentials_and_unknown_envelope_fields() {
    let json = r#"{"tool":"vendor.call","connection":"conn","operation":"read","args":{},"mode":"observe","authorization":"secret"}"#;
    assert!(serde_json::from_str::<Envelope>(json).is_err());
}
