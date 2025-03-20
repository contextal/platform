#[test]
fn folded_headers() -> Result<(), std::io::Error> {
    const SIZE_LIMIT: u64 = 10 * 1024 * 1024;
    let raw_mail = br#"From: me <me@myself>
To: "Distribution List": "Henry" <henry@example>, <dick@example>, Tom
	<tom@example>;
 <offlist@example>;
	 	<lastone@example>
Subject:
	Hello
Date: Wed,
 31 May 2023 10:17:28 +0200

Hi there!

Bye"#;
    let mut mail = email_rs::Mail::new(raw_mail.as_slice())?;
    let msg = mail.message();
    assert_eq!(msg.date().unwrap().unwrap().unix_timestamp(), 1685521048);
    assert_eq!(msg.content_type(), "text/plain");
    assert!(msg.is_text());
    assert_eq!(msg.charset(), Some("us-ascii"));
    assert!(!msg.is_multipart());
    assert_eq!(msg.content_disposition(), "inline");
    assert!(msg.is_inline());
    assert!(matches!(
        msg.transfer_encoding(),
        email_rs::TransferEncoding::SevenBit
    ));
    assert_eq!(msg.content_transfer_encoding(), None);
    assert!(!msg.has_invalid_headers());
    assert!(!msg.has_duplicate_header("from"));
    assert!(!msg.has_duplicate_header("to"));
    assert!(!msg.has_duplicate_header("subject"));
    assert!(!msg.has_duplicate_header("date"));
    assert!(!msg.is_resent());
    assert!(!msg.is_list());
    assert_eq!(msg.names().count(), 0);
    assert_eq!(
        msg.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert_eq!(msg.get_header("from").unwrap().value, "me <me@myself>");
    assert_eq!(
        msg.get_header("to").unwrap().value,
        r#""distribution list": "henry" <henry@example>, <dick@example>, tom <tom@example>; <offlist@example>; <lastone@example>"#
    );
    assert_eq!(msg.get_header("subject").unwrap().value, "hello");
    assert!(!msg.is_attachment_with_charset());

    let part = mail
        .dump_current_part(&mut std::io::sink(), SIZE_LIMIT, SIZE_LIMIT)?
        .unwrap();
    assert!(!part.has_ugly_qp);
    assert!(!part.has_ugly_b64);
    assert!(!part.unsupported_charset);
    assert!(!part.has_text_decoder_errors);
    assert!(
        mail.dump_current_part(&mut std::io::sink(), SIZE_LIMIT, SIZE_LIMIT)?
            .is_none()
    );

    assert!(
        mail.dump_current_part(&mut std::io::sink(), SIZE_LIMIT, SIZE_LIMIT)?
            .is_none()
    );
    Ok(())
}
