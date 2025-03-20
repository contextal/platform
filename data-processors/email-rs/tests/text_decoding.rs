#[test]
fn simple_mail() -> Result<(), std::io::Error> {
    const SIZE_LIMIT: u64 = 10 * 1024 * 1024;
    let raw_mail = b"\
From: me
To: You
To: You
Subject: Same line, different encoding
Mime-version: 1.0
Content-type: multipart/mixed; boundary=bound

--bound
Content-type: text/plain; charset=iso-8859-15
Content-transfer-encoding: quoted-printable
Content-disposition: inline

The euro sign: =A4=
--bound
Content-type: text/plain; charset=iso-8859-15
Content-transfer-encoding: base64
Content-disposition: inline

VGhlIGV1cm8gc2lnbjogpA==
--bound
Content-type: text/plain; charset=utf-8
Content-transfer-encoding: quoted-printable
Content-disposition: inline

The euro sign: =e2=82=ac=
--bound
Content-type: text/plain; charset=utf-8
Content-transfer-encoding: base64
Content-disposition: inline

VGhlIGV1cm8gc2lnbjog4oKs
--bound
Content-type: application/octet-stream; charset=iso-8859-15;
	name=attm.txt
Content-transfer-encoding: quoted-printable
Content-disposition: attachment; filename=\"Attachment.txt\"

The euro sign: =A4=
--bound
Content-type: application/octet-stream; charset=iso-8859-15;
	name=attm.txt
Content-transfer-encoding: base64
Content-disposition: attachment; filename=\"Attachment.txt\"

VGhlIGV1cm8gc2lnbjogpA==
--bound--
    ";
    let outref = b"The euro sign: \xe2\x82\xac";
    let mut mail = email_rs::Mail::new(raw_mail.as_slice())?;
    let msg = mail.message();
    assert_eq!(msg.date(), None);
    assert!(!msg.is_text());
    assert!(msg.is_multipart());
    assert!(!msg.has_invalid_headers());
    assert!(!msg.has_duplicate_header("from"));
    assert!(msg.has_duplicate_header("to"));
    assert!(!msg.has_duplicate_header("subject"));
    assert!(!msg.is_resent());
    assert!(!msg.is_list());

    // -- part 1
    let mut out: Vec<u8> = Vec::new();
    let dumped_part = mail
        .dump_current_part(&mut out, SIZE_LIMIT, SIZE_LIMIT)?
        .unwrap();
    assert!(dumped_part.part.is_text());
    assert!(!dumped_part.part.is_multipart());
    assert!(!dumped_part.part.has_invalid_headers());
    assert_eq!(dumped_part.part.charset(), Some("iso-8859-15"));
    assert_eq!(dumped_part.part.content_disposition(), "inline");
    assert!(dumped_part.part.is_inline());
    assert!(matches!(
        dumped_part.part.transfer_encoding(),
        email_rs::TransferEncoding::QuotedPrintable
    ));
    assert_eq!(
        dumped_part.part.content_transfer_encoding(),
        Some("quoted-printable")
    );
    assert_eq!(dumped_part.part.names().count(), 0);
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(!dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, outref);
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
    assert_eq!(dumped_part.part.charset(), Some("iso-8859-15"));
    assert_eq!(dumped_part.part.content_disposition(), "inline");
    assert!(dumped_part.part.is_inline());
    assert!(matches!(
        dumped_part.part.transfer_encoding(),
        email_rs::TransferEncoding::Base64
    ));
    assert_eq!(dumped_part.part.content_transfer_encoding(), Some("base64"));
    assert_eq!(dumped_part.part.names().count(), 0);
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(!dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, outref);
    assert!(!dumped_part.has_ugly_qp);
    assert!(!dumped_part.has_ugly_b64);
    assert!(!dumped_part.unsupported_charset);
    assert!(!dumped_part.has_text_decoder_errors);

    // -- part 3
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
        email_rs::TransferEncoding::QuotedPrintable
    ));
    assert_eq!(
        dumped_part.part.content_transfer_encoding(),
        Some("quoted-printable")
    );
    assert_eq!(dumped_part.part.names().count(), 0);
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(!dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, outref);
    assert!(dumped_part.has_ugly_qp);
    assert!(!dumped_part.has_ugly_b64);
    assert!(!dumped_part.unsupported_charset);
    assert!(!dumped_part.has_text_decoder_errors);

    // -- part 4
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
        email_rs::TransferEncoding::Base64
    ));
    assert_eq!(dumped_part.part.content_transfer_encoding(), Some("base64"));
    assert_eq!(dumped_part.part.names().count(), 0);
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(!dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, outref);
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
        email_rs::TransferEncoding::QuotedPrintable
    ));
    assert_eq!(
        dumped_part.part.content_transfer_encoding(),
        Some("quoted-printable")
    );
    assert_eq!(
        dumped_part.part.names().collect::<Vec<&str>>(),
        &["attachment.txt", "attm.txt"]
    );
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, b"The euro sign: \xa4");
    assert!(!dumped_part.has_ugly_qp);
    assert!(!dumped_part.has_ugly_b64);
    assert!(!dumped_part.unsupported_charset);
    assert!(!dumped_part.has_text_decoder_errors);

    // -- attachment 2
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
        email_rs::TransferEncoding::Base64
    ));
    assert_eq!(dumped_part.part.content_transfer_encoding(), Some("base64"));
    assert_eq!(
        dumped_part.part.names().collect::<Vec<&str>>(),
        &["attachment.txt", "attm.txt"]
    );
    assert_eq!(
        dumped_part.part.collect_header_flaws(),
        (false, false, false, false, false)
    );
    assert!(dumped_part.part.is_attachment_with_charset());
    assert_eq!(out, b"The euro sign: \xa4");
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
