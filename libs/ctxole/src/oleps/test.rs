use super::*;

fn read_oleps_property_set_stream<R: Read + Seek>(reader: &mut R) -> Result<OlePS, Error> {
    OlePS::new(reader)
}

#[test]
fn test_oleps_1() -> Result<(), Error> {
    let data: &[u8] = &[
        0xFE, 0xFF, 0x00, 0x00, 0x06, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xE0, 0x85,
        0x9F, 0xF2, 0xF9, 0x4F, 0x68, 0x10, 0xAB, 0x91, 0x08, 0x00, 0x2B, 0x27, 0xB3, 0xD9, 0x30,
        0x00, 0x00, 0x00, 0x8C, 0x01, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x98, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xA0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00,
        0x00, 0xB8, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xC4, 0x00, 0x00, 0x00, 0x05, 0x00,
        0x00, 0x00, 0xD0, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xDC, 0x00, 0x00, 0x00, 0x07,
        0x00, 0x00, 0x00, 0xE8, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0xFC, 0x00, 0x00, 0x00,
        0x09, 0x00, 0x00, 0x00, 0x10, 0x01, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x1C, 0x01, 0x00,
        0x00, 0x0A, 0x00, 0x00, 0x00, 0x3C, 0x01, 0x00, 0x00, 0x0B, 0x00, 0x00, 0x00, 0x48, 0x01,
        0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x54, 0x01, 0x00, 0x00, 0x0D, 0x00, 0x00, 0x00, 0x60,
        0x01, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00, 0x6C, 0x01, 0x00, 0x00, 0x0F, 0x00, 0x00, 0x00,
        0x74, 0x01, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x7C, 0x01, 0x00, 0x00, 0x13, 0x00, 0x00,
        0x00, 0x84, 0x01, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xE4, 0x04, 0x00, 0x00, 0x1E, 0x00,
        0x00, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x4A, 0x6F, 0x65, 0x27, 0x73, 0x20, 0x64, 0x6F, 0x63,
        0x75, 0x6D, 0x65, 0x6E, 0x74, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
        0x4A, 0x6F, 0x62, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x4A, 0x6F, 0x65,
        0x00, 0x1E, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E, 0x00,
        0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x0C,
        0x00, 0x00, 0x00, 0x4E, 0x6F, 0x72, 0x6D, 0x61, 0x6C, 0x2E, 0x64, 0x6F, 0x74, 0x6D, 0x00,
        0x1E, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x43, 0x6F, 0x72, 0x6E, 0x65, 0x6C, 0x69,
        0x75, 0x73, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x36, 0x36,
        0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00, 0x4D, 0x69, 0x63, 0x72, 0x6F,
        0x73, 0x6F, 0x66, 0x74, 0x20, 0x4F, 0x66, 0x66, 0x69, 0x63, 0x65, 0x20, 0x57, 0x6F, 0x72,
        0x64, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x6E, 0xD9, 0xA2, 0x42, 0x00, 0x00,
        0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x16, 0xD0, 0xA1, 0x4E, 0x8E, 0xC6, 0x01, 0x40, 0x00,
        0x00, 0x00, 0x00, 0x1C, 0xF2, 0xD5, 0x2A, 0xCE, 0xC6, 0x01, 0x40, 0x00, 0x00, 0x00, 0x00,
        0x3C, 0xDC, 0x73, 0xDD, 0x80, 0xC8, 0x01, 0x03, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00,
        0x03, 0x00, 0x00, 0x00, 0xE5, 0x0D, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x38, 0x4F, 0x00,
        0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let mut reader = Cursor::new(data);
    let oleps = read_oleps_property_set_stream(&mut reader)?;
    assert_eq!(oleps.byte_order, 0xFFFE);
    assert_eq!(oleps.version, 0);
    assert_eq!(oleps.system_identifier, 0x00020006);
    assert_eq!(
        oleps.clsid.to_string().to_uppercase(),
        "00000000-0000-0000-0000-000000000000"
    );
    assert_eq!(oleps.num_property_sets, 1);
    assert_eq!(oleps.fmtid.len(), 1);
    assert_eq!(oleps.offset.len(), 1);
    assert_eq!(oleps.property_set.len(), 1);
    assert_eq!(
        oleps.fmtid[0].to_string().to_uppercase(),
        "F29F85E0-4FF9-1068-AB91-08002B27B3D9"
    );
    assert_eq!(oleps.offset[0], 0x30);

    let property_set = &oleps.property_set[0];
    assert_eq!(property_set.size, 0x0000018C);
    assert_eq!(property_set.num_properties, 18);
    assert_eq!(property_set.property_identifier_and_offset.len(), 18);
    assert_eq!(property_set.property.len(), 18);

    let id_and_offset = &property_set.property_identifier_and_offset;
    assert_eq!(
        id_and_offset[0].property_identifier,
        PropertyIdentifier::Codepage
    );
    assert_eq!(id_and_offset[0].offset, 0x00000098);
    assert_eq!(
        id_and_offset[1].property_identifier,
        PropertyIdentifier::Normal(0x00000002)
    );
    assert_eq!(id_and_offset[1].offset, 0x000000A0);
    assert_eq!(
        id_and_offset[2].property_identifier,
        PropertyIdentifier::Normal(0x00000003)
    );
    assert_eq!(id_and_offset[2].offset, 0x000000B8);
    assert_eq!(
        id_and_offset[3].property_identifier,
        PropertyIdentifier::Normal(0x00000004)
    );
    assert_eq!(id_and_offset[3].offset, 0x000000C4);
    assert_eq!(
        id_and_offset[4].property_identifier,
        PropertyIdentifier::Normal(0x00000005)
    );
    assert_eq!(id_and_offset[4].offset, 0x000000D0);
    assert_eq!(
        id_and_offset[5].property_identifier,
        PropertyIdentifier::Normal(0x00000006)
    );
    assert_eq!(id_and_offset[5].offset, 0x000000DC);
    assert_eq!(
        id_and_offset[6].property_identifier,
        PropertyIdentifier::Normal(0x00000007)
    );
    assert_eq!(id_and_offset[6].offset, 0x000000E8);
    assert_eq!(
        id_and_offset[7].property_identifier,
        PropertyIdentifier::Normal(0x00000008)
    );
    assert_eq!(id_and_offset[7].offset, 0x000000FC);
    assert_eq!(
        id_and_offset[8].property_identifier,
        PropertyIdentifier::Normal(0x00000009)
    );
    assert_eq!(id_and_offset[8].offset, 0x00000110);
    assert_eq!(
        id_and_offset[9].property_identifier,
        PropertyIdentifier::Normal(0x00000012)
    );
    assert_eq!(id_and_offset[9].offset, 0x0000011C);
    assert_eq!(
        id_and_offset[10].property_identifier,
        PropertyIdentifier::Normal(0x0000000A)
    );
    assert_eq!(id_and_offset[10].offset, 0x0000013C);
    assert_eq!(
        id_and_offset[11].property_identifier,
        PropertyIdentifier::Normal(0x0000000B)
    );
    assert_eq!(id_and_offset[11].offset, 0x00000148);
    assert_eq!(
        id_and_offset[12].property_identifier,
        PropertyIdentifier::Normal(0x0000000C)
    );
    assert_eq!(id_and_offset[12].offset, 0x00000154);
    assert_eq!(
        id_and_offset[13].property_identifier,
        PropertyIdentifier::Normal(0x0000000D)
    );
    assert_eq!(id_and_offset[13].offset, 0x00000160);
    assert_eq!(
        id_and_offset[14].property_identifier,
        PropertyIdentifier::Normal(0x0000000E)
    );
    assert_eq!(id_and_offset[14].offset, 0x0000016C);
    assert_eq!(
        id_and_offset[15].property_identifier,
        PropertyIdentifier::Normal(0x0000000F)
    );
    assert_eq!(id_and_offset[15].offset, 0x00000174);
    assert_eq!(
        id_and_offset[16].property_identifier,
        PropertyIdentifier::Normal(0x00000010)
    );
    assert_eq!(id_and_offset[16].offset, 0x0000017C);
    assert_eq!(
        id_and_offset[17].property_identifier,
        PropertyIdentifier::Normal(0x00000013)
    );
    assert_eq!(id_and_offset[17].offset, 0x00000184);

    let property = &property_set.property;
    if let Property::TypedPropertyValue(TypedPropertyValue::I2(codepage)) = &property[0] {
        assert_eq!(*codepage, 0x04E4);
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[1] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "Joe's document");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[2] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "Job");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[3] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "Joe");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[4] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "\0\0\0");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[5] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "\0\0\0");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[6] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "Normal.dotm");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[7] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "Cornelius");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[8] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "66\0");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::LPStr(codepage_string)) = &property[9] {
        assert!(!codepage_string.is_winunicode());
        assert_eq!(codepage_string.to_string(), "Microsoft Office Word\0\0");
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::Filetime(filetime)) = &property[10] {
        let duration = filetime.as_duration().expect("Valid duration expected");
        assert_eq!(duration, time::Duration::seconds((7 * 60 + 57) * 60));
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::Filetime(filetime)) = &property[11] {
        let datetime = filetime.as_datetime().expect("Valid datetime expected");
        let expected = time::OffsetDateTime::new_utc(
            time::Date::from_calendar_date(2006, time::Month::June, 12).unwrap(),
            time::Time::from_hms(18, 33, 00).unwrap(),
        );
        assert_eq!(datetime, expected);
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::Filetime(filetime)) = &property[12] {
        let datetime = filetime.as_datetime().expect("Valid datetime expected");
        let expected = time::OffsetDateTime::new_utc(
            time::Date::from_calendar_date(2006, time::Month::September, 2).unwrap(),
            time::Time::from_hms(0, 58, 00).unwrap(),
        );
        assert_eq!(datetime, expected);
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::Filetime(filetime)) = &property[13] {
        let datetime = filetime.as_datetime().expect("Valid datetime expected");
        let expected = time::OffsetDateTime::new_utc(
            time::Date::from_calendar_date(2008, time::Month::March, 8).unwrap(),
            time::Time::from_hms(5, 30, 00).unwrap(),
        );
        assert_eq!(datetime, expected);
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::I4(value)) = &property[14] {
        assert_eq!(*value, 14);
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::I4(value)) = &property[15] {
        assert_eq!(*value, 3557);
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::I4(value)) = &property[16] {
        assert_eq!(*value, 20_280);
    } else {
        panic!("Invalid Property");
    }
    if let Property::TypedPropertyValue(TypedPropertyValue::I4(value)) = &property[17] {
        assert_eq!(*value, 0);
    } else {
        panic!("Invalid Property");
    }
    Ok(())
}

#[test]
fn test_oleps_2() -> Result<(), Error> {
    use std::io::Cursor;
    let data: &[u8] = &[
        0xFE, 0xFF, 0x01, 0x00, 0x06, 0x00, 0x02, 0x00, 0x53, 0xFF, 0x4B, 0x99, 0xF9, 0xDD, 0xAD,
        0x42, 0xA5, 0x6A, 0xFF, 0xEA, 0x36, 0x17, 0xAC, 0x16, 0x01, 0x00, 0x00, 0x00, 0x01, 0x18,
        0x00, 0x20, 0xE6, 0x5D, 0xD1, 0x11, 0x8E, 0x38, 0x00, 0xC0, 0x4F, 0xB9, 0x38, 0x6D, 0x30,
        0x00, 0x00, 0x00, 0xDC, 0x01, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x58, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x60, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x80, 0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x00, 0x00, 0x04, 0x00,
        0x00, 0x00, 0x38, 0x01, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x4C, 0x01, 0x00, 0x00, 0x07,
        0x00, 0x00, 0x00, 0x70, 0x01, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x7C, 0x01, 0x00, 0x00,
        0x27, 0x00, 0x00, 0x00, 0x94, 0x01, 0x00, 0x00, 0x92, 0x00, 0x00, 0x00, 0xC0, 0x01, 0x00,
        0x00, 0x02, 0x00, 0x00, 0x00, 0xB0, 0x04, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x09, 0x08, 0x13, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x04,
        0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00, 0x44, 0x00, 0x69, 0x00, 0x73, 0x00, 0x70, 0x00,
        0x6C, 0x00, 0x61, 0x00, 0x79, 0x00, 0x43, 0x00, 0x6F, 0x00, 0x6C, 0x00, 0x6F, 0x00, 0x75,
        0x00, 0x72, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x4D, 0x00,
        0x79, 0x00, 0x53, 0x00, 0x74, 0x00, 0x72, 0x00, 0x65, 0x00, 0x61, 0x00, 0x6D, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x00, 0x00, 0x50, 0x00, 0x72, 0x00,
        0x69, 0x00, 0x63, 0x00, 0x65, 0x00, 0x28, 0x00, 0x47, 0x00, 0x42, 0x00, 0x50, 0x00, 0x29,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x4D, 0x00,
        0x79, 0x00, 0x53, 0x00, 0x74, 0x00, 0x6F, 0x00, 0x72, 0x00, 0x61, 0x00, 0x67, 0x00, 0x65,
        0x00, 0x00, 0x00, 0x27, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00, 0x43, 0x00, 0x61, 0x00,
        0x73, 0x00, 0x65, 0x00, 0x53, 0x00, 0x65, 0x00, 0x6E, 0x00, 0x73, 0x00, 0x69, 0x00, 0x74,
        0x00, 0x69, 0x00, 0x76, 0x00, 0x65, 0x00, 0x00, 0x00, 0x92, 0x00, 0x00, 0x00, 0x0E, 0x00,
        0x00, 0x00, 0x43, 0x00, 0x41, 0x00, 0x53, 0x00, 0x45, 0x00, 0x53, 0x00, 0x45, 0x00, 0x4E,
        0x00, 0x53, 0x00, 0x49, 0x00, 0x54, 0x00, 0x49, 0x00, 0x56, 0x00, 0x45, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x47, 0x00, 0x72, 0x00, 0x65, 0x00, 0x79,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x49, 0x00, 0x00, 0x00, 0xCA, 0x84, 0x95, 0xF9, 0x23, 0xCA,
        0x0B, 0x47, 0x83, 0x94, 0x22, 0x01, 0x77, 0x90, 0x7A, 0xAD, 0x0C, 0x00, 0x00, 0x00, 0x70,
        0x00, 0x72, 0x00, 0x6F, 0x00, 0x70, 0x00, 0x36, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00,
        0x00, 0x50, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x45, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x00,
        0x00, 0x70, 0x00, 0x72, 0x00, 0x6F, 0x00, 0x70, 0x00, 0x31, 0x00, 0x32, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x10, 0x20, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03,
        0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x03, 0xF8, 0x14, 0x17, 0x12, 0x87, 0x45, 0x29, 0x25, 0x11, 0x33, 0x56, 0x79, 0xA2, 0x9C,
        0x00, 0x0C, 0x10, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, 0xA9, 0x00,
        0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0xA9, 0x00, 0x76, 0x99, 0x3B, 0x22, 0x10, 0x9C,
    ];

    let mut reader = Cursor::new(data);
    let oleps = read_oleps_property_set_stream(&mut reader)?;
    assert_eq!(oleps.byte_order, 0xFFFE);
    assert_eq!(oleps.version, 1);
    assert_eq!(oleps.system_identifier, 0x00020006);
    assert_eq!(
        oleps.clsid.to_string().to_uppercase(),
        "994BFF53-DDF9-42AD-A56A-FFEA3617AC16"
    );
    assert_eq!(oleps.num_property_sets, 1);
    assert_eq!(oleps.fmtid.len(), 1);
    assert_eq!(oleps.offset.len(), 1);
    assert_eq!(oleps.property_set.len(), 1);

    assert_eq!(
        oleps.fmtid[0].to_string().to_uppercase(),
        "20001801-5DE6-11D1-8E38-00C04FB9386D"
    );
    assert_eq!(oleps.offset[0], 0x30);

    let property_set = &oleps.property_set[0];
    assert_eq!(property_set.size, 0x000001DC);
    assert_eq!(property_set.num_properties, 10);
    assert_eq!(property_set.property_identifier_and_offset.len(), 10);
    assert_eq!(property_set.property.len(), 10);

    let id_and_offset = &property_set.property_identifier_and_offset;
    assert_eq!(
        id_and_offset[0].property_identifier,
        PropertyIdentifier::Codepage
    );
    assert_eq!(id_and_offset[0].offset, 0x00000058);
    assert_eq!(
        id_and_offset[1].property_identifier,
        PropertyIdentifier::Locale
    );
    assert_eq!(id_and_offset[1].offset, 0x00000060);
    assert_eq!(
        id_and_offset[2].property_identifier,
        PropertyIdentifier::Behavior
    );
    assert_eq!(id_and_offset[2].offset, 0x00000068);
    assert_eq!(
        id_and_offset[3].property_identifier,
        PropertyIdentifier::Dictionary
    );
    assert_eq!(id_and_offset[3].offset, 0x00000070);
    assert_eq!(
        id_and_offset[4].property_identifier,
        PropertyIdentifier::Normal(0x00000004)
    );
    assert_eq!(id_and_offset[4].offset, 0x00000138);
    assert_eq!(
        id_and_offset[5].property_identifier,
        PropertyIdentifier::Normal(0x00000006)
    );
    assert_eq!(id_and_offset[5].offset, 0x0000014C);
    assert_eq!(
        id_and_offset[6].property_identifier,
        PropertyIdentifier::Normal(0x00000007)
    );
    assert_eq!(id_and_offset[6].offset, 0x00000170);
    assert_eq!(
        id_and_offset[7].property_identifier,
        PropertyIdentifier::Normal(0x0000000c)
    );
    assert_eq!(id_and_offset[7].offset, 0x0000017C);
    assert_eq!(
        id_and_offset[8].property_identifier,
        PropertyIdentifier::Normal(0x00000027)
    );
    assert_eq!(id_and_offset[8].offset, 0x00000194);
    assert_eq!(
        id_and_offset[9].property_identifier,
        PropertyIdentifier::Normal(0x00000092)
    );
    assert_eq!(id_and_offset[9].offset, 0x000001C0);

    let property = &property_set.property;
    let codepage = match &property[0] {
        Property::TypedPropertyValue(TypedPropertyValue::I2(value)) => *value,
        _ => panic!("Invalid Property"),
    };
    assert_eq!(codepage, 0x04B0);
    let locale = match &property[1] {
        Property::TypedPropertyValue(TypedPropertyValue::UI4(value)) => *value,
        _ => panic!("Invalid Property"),
    };
    assert!(locale == 0x08090000);
    let behavior = match &property[2] {
        Property::TypedPropertyValue(TypedPropertyValue::UI4(value)) => *value,
        _ => panic!("Invalid Property"),
    };
    assert!(behavior == 1);
    let dictionary = match &property[3] {
        Property::Dictionary(dictionary) => dictionary,
        _ => panic!("Invalid Property"),
    };
    assert_eq!(dictionary.num_entries, 6);
    assert_eq!(dictionary.entry.len(), 6);
    assert_eq!(
        dictionary.entry[0].property_identifier,
        PropertyIdentifier::Normal(4)
    );
    assert!(dictionary.entry[0].name.is_winunicode());
    assert_eq!(dictionary.entry[0].name.to_string(), "DisplayColour");
    assert_eq!(
        dictionary.entry[1].property_identifier,
        PropertyIdentifier::Normal(6)
    );
    assert!(dictionary.entry[1].name.is_winunicode());
    assert_eq!(dictionary.entry[1].name.to_string(), "MyStream");
    assert_eq!(
        dictionary.entry[2].property_identifier,
        PropertyIdentifier::Normal(7)
    );
    assert!(dictionary.entry[2].name.is_winunicode());
    assert_eq!(dictionary.entry[2].name.to_string(), "Price(GBP)");
    assert_eq!(
        dictionary.entry[3].property_identifier,
        PropertyIdentifier::Normal(12)
    );
    assert!(dictionary.entry[3].name.is_winunicode());
    assert_eq!(dictionary.entry[3].name.to_string(), "MyStorage");
    assert_eq!(
        dictionary.entry[4].property_identifier,
        PropertyIdentifier::Normal(39)
    );
    assert!(dictionary.entry[4].name.is_winunicode());
    assert_eq!(dictionary.entry[4].name.to_string(), "CaseSensitive");
    assert_eq!(
        dictionary.entry[5].property_identifier,
        PropertyIdentifier::Normal(146)
    );
    assert!(dictionary.entry[5].name.is_winunicode());
    assert_eq!(dictionary.entry[5].name.to_string(), "CASESENSITIVE");

    let display_colour = match &property[4] {
        Property::TypedPropertyValue(TypedPropertyValue::BStr(value)) => value,
        _ => panic!("Invalid Property"),
    };
    assert!(display_colour.is_winunicode());
    assert_eq!(display_colour.to_string(), "Grey");

    let my_stream = match &property[5] {
        Property::TypedPropertyValue(TypedPropertyValue::VersionedStream(value)) => value,
        _ => panic!("Invalid Property"),
    };
    assert_eq!(
        my_stream.version_guid.to_string().to_uppercase(),
        "F99584CA-CA23-470B-8394-220177907AAD"
    );
    assert!(my_stream.stream_name.is_winunicode());
    assert_eq!(my_stream.stream_name.to_string(), "prop6");

    let price_gbp = match &property[6] {
        Property::TypedPropertyValue(TypedPropertyValue::CY(value)) => value,
        _ => panic!("Invalid Property"),
    };
    assert_eq!(price_gbp.value, 133_1200);

    let my_storage = match &property[7] {
        Property::TypedPropertyValue(TypedPropertyValue::StoredObject(value)) => value,
        _ => panic!("Invalid property"),
    };
    assert!(my_storage.is_winunicode());
    assert_eq!(my_storage.to_string(), "prop12");

    let array = match &property[8] {
        Property::TypedPropertyValue(TypedPropertyValue::ArrayI1(value)) => value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(array.header.value_type, 0x00000010);
    assert_eq!(array.header.num_dimensions, 2);
    assert_eq!(array.dimensions.len(), 2);
    assert_eq!(array.dimensions[0].size, 3);
    assert_eq!(array.dimensions[0].index_offset, -1);
    assert_eq!(array.dimensions[1].size, 5);
    assert_eq!(array.dimensions[1].index_offset, 0);

    assert_eq!(array.data.len(), 15);
    assert_eq!(array.data[0], 3);
    assert_eq!(array.data[1], -8);
    assert_eq!(array.data[2], 20);
    assert_eq!(array.data[3], 23);
    assert_eq!(array.data[4], 18);
    assert_eq!(array.data[5], -121);
    assert_eq!(array.data[6], 69);
    assert_eq!(array.data[7], 41);
    assert_eq!(array.data[8], 37);
    assert_eq!(array.data[9], 17);
    assert_eq!(array.data[10], 51);
    assert_eq!(array.data[11], 86);
    assert_eq!(array.data[12], 121);
    assert_eq!(array.data[13], -94);
    assert_eq!(array.data[14], -100);

    let vector = match &property[9] {
        Property::TypedPropertyValue(TypedPropertyValue::VectorVariant(value)) => value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(vector.data.len(), 2);

    let entry0 = match &vector.data[0] {
        TypedPropertyValue::UI1(value) => *value,
        _ => panic!("Invalid vector entry"),
    };
    assert_eq!(entry0, 169);
    let entry1 = match &vector.data[1] {
        TypedPropertyValue::I8(value) => *value,
        _ => panic!("Invalid vector entry"),
    };
    assert_eq!(entry1, -7201218164792360791);
    Ok(())
}

#[test]
fn test_oleps_3() -> Result<(), Error> {
    use std::io::Cursor;
    #[rustfmt::skip]
    let data: [u8; 0x15c] = [
        /* 0x0000 */ 0xfe,0xff, // ByteOrder
        /* 0x0002 */ 0x00,0x00, // Version
        /* 0x0004 */ 0x06,0x00,0x02,0x00, // SystemIdentifier
        /* 0x0008 */ 0xf0,0xe1,0xd2,0xc3,0xb4,0xa5,0x86,0x87, // CLSID
        /* 0x0010 */ 0x78,0x69,0x5a,0x4b,0x3c,0x2d,0x1e,0x0f, // CLSID
        /* 0x0018 */ 0x01,0x00,0x00,0x00, // NumPropertySets
        /* 0x001c */ 0xe0,0x85,0x9f,0xf2,0xf9,0x4f,0x68,0x10, // FMTID0
        /* 0x0024 */ 0xab,0x91,0x08,0x00,0x2b,0x27,0xb3,0xd9, // FMTID0
        /* 0x002c */ 0x30,0x00,0x00,0x00, // Offset0
        /* 0x0030 */ 0x2c,0x01,0x00,0x00, // Size
        /* 0x0034 */ 0x0f,0x00,0x00,0x00, // NumProperties
        /* 0x0038 */ 0x00,0x00,0x37,0x13,0x80,0x00,0x00,0x00, // Empty[1]
        /* 0x0040 */ 0x01,0x00,0x00,0x00,0x88,0x00,0x00,0x00, // CodePage[2]
        /* 0x0048 */ 0x01,0x00,0x37,0x13,0x90,0x00,0x00,0x00, // Signed[3]
        /* 0x0050 */ 0x02,0x00,0x37,0x13,0x98,0x00,0x00,0x00, // Float[4]
        /* 0x0058 */ 0x03,0x00,0x37,0x13,0xa0,0x00,0x00,0x00, // Unsigned[5]
        /* 0x0060 */ 0x04,0x00,0x37,0x13,0xa8,0x00,0x00,0x00, // Decimal[6]
        /* 0x0068 */ 0x05,0x00,0x37,0x13,0xbc,0x00,0x00,0x00, // Unsigned[7]
        /* 0x0070 */ 0x06,0x00,0x37,0x13,0xc8,0x00,0x00,0x00, // String[8]
        /* 0x0078 */ 0x07,0x00,0x37,0x13,0xd8,0x00,0x00,0x00, // Null[9]
        /* 0x0080 */ 0x08,0x00,0x37,0x13,0xe0,0x00,0x00,0x00, // LPWSTR[10]
        /* 0x0088 */ 0x09,0x00,0x37,0x13,0xf0,0x00,0x00,0x00, // Bool[11]
        /* 0x0090 */ 0x0a,0x00,0x37,0x13,0xf8,0x00,0x00,0x00, // Bool[12]
        /* 0x0098 */ 0x0b,0x00,0x37,0x13,0x00,0x01,0x00,0x00, // FILETIME[13]
        /* 0x00a0 */ 0x0c,0x00,0x37,0x13,0x0c,0x01,0x00,0x00, // BLOB[14]
        /* 0x00a8 */ 0x0d,0x00,0x37,0x13,0x20,0x01,0x00,0x00, // CY[15]
        /* 0x00b0 */ 0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00, // [1] EMPTY
        /* 0x00b8 */ 0x02,0x00,0x00,0x00,0x52,0x03,0x00,0x00, // [2] I2(850)
        /* 0x00c0 */ 0x03,0x00,0x00,0x00,0xab,0xac,0x37,0x13, // [3] I4(0x1337acab)
        /* 0x00c8 */ 0x04,0x00,0x00,0x00,0xc3,0xf5,0x48,0x40, // [4] R4(3.14)
        /* 0x00d0 */ 0x11,0x00,0x00,0x00,0xa5,0x00,0x00,0x00, // [5] UI1(0xa5)
        /* 0x00d8 */ 0x0e,0x00,0x00,0x00,0x00,0x00,0x0a,0x80, // [6] DECIMAL(-9876543210.0123456789)
        /* 0x00e0 */ 0x05,0x00,0x00,0x00,0x15,0x35,0xd2,0x9f,
        /* 0x00e8 */ 0x36,0x4d,0xa5,0x5a,
        /* 0x00ec */ 0x15,0x00,0x00,0x00,0x0d,0xd0,0x37,0x13, // [7] UI8(0xdeadbeef1337d00d)
        /* 0x00f4 */ 0xef,0xbe,0xad,0xde,
        /* 0x00f8 */ 0x1e,0x00,0x00,0x00,0x07,0x00,0x00,0x00, // [8] LPSTR - UTF-8(ABC123)
        /* 0x0100 */ 0x41,0x42,0x43,0x31,0x32,0x33,0x00,0x00,
        /* 0x0108 */ 0x01,0x00,0x00,0x00,0x00,0x00,0x00,0x00, // [9] NULL
        /* 0x0110 */ 0x1f,0x00,0x00,0x00,0x04,0x00,0x00,0x00, // [10] LPWSTR("←⏭➘")
        /* 0x0118 */ 0x90,0x21,0xed,0x23,0x98,0x27,0x00,0x00,
        /* 0x0120 */ 0x0b,0x00,0x00,0x00,0x01,0x00,0x00,0x00, // [11] Bool(true)
        /* 0x0128 */ 0x0b,0x00,0x00,0x00,0x00,0x00,0x00,0x00, // [12] Bool(false)
        /* 0x0130 */ 0x40,0x00,0x00,0x00,0x80,0x80,0xf4,0xed, // [13] DateTime(2021-01-02T03:04:05)
        /* 0x0138 */ 0xb3,0xe0,0xd6,0x01,
        /* 0x013c */ 0x41,0x00,0x00,0x00,0x09,0x00,0x00,0x00, // [14] Unsupported(BLOB[1..9])
        /* 0x0144 */ 0x01,0x02,0x03,0x04,0x05,0x06,0x07,0x08,
        /* 0x014c */ 0x09,0x0a,0x00,0x00,
        /* 0x0150 */ 0x06,0x00,0x00,0x00,0x00,0xe4,0x0b,0x54, // [15] Decimal(1mln)
        /* 0x0158 */ 0x02,0x00,0x00,0x00
    ];

    let mut reader = Cursor::new(data);
    let oleps = read_oleps_property_set_stream(&mut reader)?;
    assert_eq!(oleps.byte_order, 0xFFFE);

    // OlePS
    assert_eq!(oleps.version, 0);
    assert_eq!(oleps.system_identifier, 0x00020006);
    assert_eq!(
        oleps.clsid.to_string().to_lowercase(),
        "c3d2e1f0-a5b4-8786-7869-5a4b3c2d1e0f"
    );
    assert_eq!(oleps.num_property_sets, 1);
    assert_eq!(oleps.property_set.len(), 1);
    assert_eq!(oleps.fmtid.len(), 1);
    assert_eq!(oleps.offset.len(), 1);

    // PropertySet
    assert_eq!(
        oleps.fmtid[0].to_string().to_lowercase(),
        "f29f85e0-4ff9-1068-ab91-08002b27b3d9"
    );
    assert_eq!(oleps.property_set[0].num_properties, 15);
    assert_eq!(oleps.property_set[0].property.len(), 15);
    assert_eq!(
        oleps.property_set[0].property_identifier_and_offset.len(),
        15
    );

    // [1]
    let p = &oleps.property_set[0].property[0];
    let i = &oleps.property_set[0].property_identifier_and_offset[0];
    assert_eq!(
        i.property_identifier,
        PropertyIdentifier::Normal(0x13370000)
    );
    match p {
        Property::TypedPropertyValue(TypedPropertyValue::Empty) => (),
        _ => panic!("Invalid property"),
    }

    // [2]
    let p = &oleps.property_set[0].property[1];
    let i = &oleps.property_set[0].property_identifier_and_offset[1];
    assert_eq!(i.property_identifier, PropertyIdentifier::Codepage);
    let codepage = match p {
        Property::TypedPropertyValue(TypedPropertyValue::I2(value)) => *value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(codepage, 850);

    // [3]
    let p = &oleps.property_set[0].property[2];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::I4(value)) => *value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(value, 0x1337acab);

    // [4]
    let p = &oleps.property_set[0].property[3];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::R4(value)) => *value,
        _ => panic!("Invalid property"),
    };
    #[allow(clippy::approx_constant)]
    let expected = 3.14f32;
    assert_eq!(value, expected);

    // [5]
    let p = &oleps.property_set[0].property[4];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::UI1(value)) => *value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(value, 0xa5);

    // [6]
    let p = &oleps.property_set[0].property[5];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::Decimal(value)) => value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(
        *value,
        Decimal {
            value: -98765432100123456789i128,
            scale: 10
        }
    );

    // [7]
    let p = &oleps.property_set[0].property[6];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::UI8(value)) => *value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(value, 0xdeadbeef1337d00d);

    // [8]
    let p = &oleps.property_set[0].property[7];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::LPStr(value)) => value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(value.to_string(), String::from("ABC123"));

    // [9]
    let p = &oleps.property_set[0].property[8];
    match p {
        Property::TypedPropertyValue(TypedPropertyValue::Null) => (),
        _ => panic!("Invalid property"),
    }

    // [10]
    let p = &oleps.property_set[0].property[9];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::LPWStr(value)) => value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(value.to_string(), String::from("←⏭➘"));

    // [11]
    let p = &oleps.property_set[0].property[10];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::Bool(value)) => *value,
        _ => panic!("Invalid property"),
    };
    assert!(value);

    // [12]
    let p = &oleps.property_set[0].property[11];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::Bool(value)) => *value,
        _ => panic!("Invalid property"),
    };
    assert!(!value);

    // [13]
    let p = &oleps.property_set[0].property[12];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::Filetime(value)) => value,
        _ => panic!("Invalid property"),
    };
    let datetime = value.as_datetime().unwrap();
    let expected = time::OffsetDateTime::new_utc(
        time::Date::from_calendar_date(2021, time::Month::January, 2).unwrap(),
        time::Time::from_hms(3, 4, 5).unwrap(),
    );
    assert_eq!(datetime, expected);

    // [14]
    let p = &oleps.property_set[0].property[13];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::Blob(value)) => value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(value.bytes, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);

    // [15]
    let p = &oleps.property_set[0].property[14];
    let value = match p {
        Property::TypedPropertyValue(TypedPropertyValue::CY(value)) => value,
        _ => panic!("Invalid property"),
    };
    assert_eq!(value.value, 10_000_000_000);

    Ok(())
}
