#[test]
fn multipart_mail() -> Result<(), std::io::Error> {
    const SIZE_LIMIT: u64 = 10 * 1024 * 1024;
    let raw_mail = b"\
From: me <me@myself>
To: you
Subject: mail
Date: Mon, 12 Jun 2023 09:09:42 GMT
Content-Type: multipart/mixed; boundary=outer

Outer preamble
--outer
Content-Type: multipart/alternative; boundary=\"inner\"

Inner preamble
--inner
Content-Type: text/plain; charset=utf-8
Content-Transfer-Encoding: 7bit

Text
--inner
Content-type: text/html

<p>Html</p>
--inner--
Inner epilogue

--outer
Content-type: application/octet-stream
Content-Transfer-Encoding: binary
Content-disposition: attachment;
	filename=\"\\\"quoted\\\".bin

Binary data
--outer--
Epilogue
    ";
    let mut mail = email_rs::Mail::new(raw_mail.as_slice())?;
    let msg = mail.message();
    assert_eq!(msg.date().unwrap().unwrap().unix_timestamp(), 1686560982);
    assert!(!msg.is_text());
    assert!(msg.is_multipart());
    assert!(!msg.has_invalid_headers());

    // -- part 1
    let mut out: Vec<u8> = Vec::new();
    let dumped_part = mail
        .dump_current_part(&mut out, SIZE_LIMIT, SIZE_LIMIT)?
        .unwrap();
    assert!(dumped_part.part.is_text());
    assert!(!dumped_part.part.is_multipart());
    assert!(!dumped_part.part.has_invalid_headers());
    assert_eq!(dumped_part.part.charset(), Some("utf-8"));
    assert_eq!(dumped_part.part.content_disposition(), "inline");
    assert!(dumped_part.part.is_inline());
    assert!(matches!(
        dumped_part.part.transfer_encoding(),
        email_rs::TransferEncoding::SevenBit
    ));
    assert_eq!(dumped_part.part.content_transfer_encoding(), Some("7bit"));
    assert_eq!(dumped_part.part.names().count(), 0);
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(!dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, b"Text\n");
    assert!(!dumped_part.has_ugly_qp);
    assert!(!dumped_part.has_ugly_b64);
    assert!(!dumped_part.unsupported_charset);
    assert!(!dumped_part.has_text_decoder_errors);

    // -- part 2
    let mut out: Vec<u8> = Vec::new();
    let dumped_part = mail
        .dump_current_part(&mut out, SIZE_LIMIT, SIZE_LIMIT)?
        .unwrap();
    assert!(dumped_part.part.is_text());
    assert!(!dumped_part.part.is_multipart());
    assert!(!dumped_part.part.has_invalid_headers());
    assert_eq!(dumped_part.part.charset(), Some("us-ascii"));
    assert_eq!(dumped_part.part.content_disposition(), "inline");
    assert!(dumped_part.part.is_inline());
    assert!(matches!(
        dumped_part.part.transfer_encoding(),
        email_rs::TransferEncoding::SevenBit
    ));
    assert_eq!(dumped_part.part.content_transfer_encoding(), None);
    assert_eq!(dumped_part.part.names().count(), 0);
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(!dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, b"<p>Html</p>\n");
    assert!(!dumped_part.has_ugly_qp);
    assert!(!dumped_part.has_ugly_b64);
    assert!(!dumped_part.unsupported_charset);
    assert!(!dumped_part.has_text_decoder_errors);

    // -- attachment 1
    let mut out: Vec<u8> = Vec::new();
    let dumped_part = mail
        .dump_current_part(&mut out, SIZE_LIMIT, SIZE_LIMIT)?
        .unwrap();
    assert!(!dumped_part.part.is_text());
    assert!(!dumped_part.part.is_multipart());
    assert!(!dumped_part.part.has_invalid_headers());
    assert_eq!(dumped_part.part.charset(), None);
    assert_eq!(dumped_part.part.content_disposition(), "attachment");
    assert!(!dumped_part.part.is_inline());
    assert!(matches!(
        dumped_part.part.transfer_encoding(),
        email_rs::TransferEncoding::Binary
    ));
    assert_eq!(dumped_part.part.content_transfer_encoding(), Some("binary"));
    assert_eq!(dumped_part.part.names().next().unwrap(), "\"quoted\".bin");
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(!dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, b"Binary data\n");
    assert!(!dumped_part.has_ugly_qp);
    assert!(!dumped_part.has_ugly_b64);
    assert!(!dumped_part.unsupported_charset);
    assert!(!dumped_part.has_text_decoder_errors);

    assert!(
        mail.dump_current_part(&mut std::io::sink(), SIZE_LIMIT, SIZE_LIMIT)?
            .is_none()
    );
    Ok(())
}
