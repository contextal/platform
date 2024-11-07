#[test]
fn simple_mail() -> Result<(), std::io::Error> {
    const SIZE_LIMIT: u64 = 10 * 1024 * 1024;
    let raw_mail = b"\
    From: me <me@myself>\r\n\
    To: you\r\n\
    Subject: test \t mail \r\n\
    Date: Wed, 31 May 2023 10:17:28 +0200\r\n\
    \r\n\
    Hi there!\r\n\
    \r\n\
    Bye\
    ";
    let mut mail = email_rs::Mail::new(raw_mail.as_slice())?;
    let mut out: Vec<u8> = Vec::new();
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
    assert_eq!(msg.get_header("to").unwrap().value, "you");
    assert_eq!(msg.get_header("subject").unwrap().value, "test mail");
    assert!(!msg.is_attachment_with_charset());

    let part = mail
        .dump_current_part(&mut out, SIZE_LIMIT, SIZE_LIMIT)?
        .unwrap();
    assert_eq!(out, b"Hi there!\n\nBye\n");
    assert!(!part.has_ugly_qp);
    assert!(!part.has_ugly_b64);
    assert!(!part.unsupported_charset);
    assert!(!part.has_text_decoder_errors);

    assert!(mail
        .dump_current_part(&mut std::io::sink(), SIZE_LIMIT, SIZE_LIMIT)?
        .is_none());
    Ok(())
}
