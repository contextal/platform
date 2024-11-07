use crate::PdfBackendError;
use serde::Serialize;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};
use tracing::warn;

/// PDF date string, as defined in The PDF Reference Manual, section 3.8.3
#[derive(Debug, Serialize)]
pub struct BackendDateTime {
    /// PDF date string in the original form, which might violate standard requirements.
    raw: String,

    /// Parsed PDF date string or `None` if parsing has failed.
    pub parsed: Option<OffsetDateTime>,
}

impl From<&str> for BackendDateTime {
    fn from(raw: &str) -> Self {
        use nom::{
            bytes::complete::{tag, take_while_m_n},
            character::complete::one_of,
            combinator::{all_consuming, map_res, opt, verify},
            sequence::{delimited, tuple},
            AsChar, Parser,
        };

        let parsed = (|| -> Result<OffsetDateTime, PdfBackendError> {
            let prefix = tag::<&str, &str, nom::error::Error<&str>>("D:");
            let year = map_res(take_while_m_n(4, 4, AsChar::is_dec_digit), |s: &str| {
                s.parse::<i32>()
            });
            let month = verify(
                map_res(take_while_m_n(2, 2, AsChar::is_dec_digit), |s: &str| {
                    s.parse::<u8>()
                }),
                |month| (1..=12).contains(month),
            );
            let day = verify(
                map_res(take_while_m_n(2, 2, AsChar::is_dec_digit), |s: &str| {
                    s.parse::<u8>()
                }),
                |day| (1..=31).contains(day),
            );
            let hour = verify(
                map_res(take_while_m_n(2, 2, AsChar::is_dec_digit), |s: &str| {
                    s.parse::<u8>()
                }),
                |hour| (0..=23).contains(hour),
            );
            let minute = verify(
                map_res(take_while_m_n(2, 2, AsChar::is_dec_digit), |s: &str| {
                    s.parse::<u8>()
                }),
                |minute| (0..=59).contains(minute),
            );
            let second = verify(
                map_res(take_while_m_n(2, 2, AsChar::is_dec_digit), |s: &str| {
                    s.parse::<u8>()
                }),
                |second| (0..=59).contains(second),
            );
            let offset_direction = one_of("+-Z");
            let offset_hour = verify(
                map_res(take_while_m_n(2, 2, AsChar::is_dec_digit), |s: &str| {
                    s.parse::<i8>()
                }),
                |hour| (0..=23).contains(hour),
            );
            let offset_minute = verify(
                map_res(
                    delimited(
                        tag("'"),
                        take_while_m_n(2, 2, AsChar::is_dec_digit),
                        tag("'"),
                    ),
                    |s: &str| s.parse::<i8>(),
                ),
                |minute| (0..=59).contains(minute),
            );

            let (_remainder, (_prefix, year, rest)) = all_consuming(tuple((
                opt(prefix),
                year,
                opt(tuple((
                    month,
                    opt(tuple((
                        day,
                        opt(tuple((
                            hour,
                            opt(tuple((
                                minute,
                                opt(tuple((
                                    second,
                                    opt(tuple((
                                        offset_direction,
                                        opt(tuple((offset_hour, opt(offset_minute)))),
                                    ))),
                                ))),
                            ))),
                        ))),
                    ))),
                ))),
            )))
            .parse(raw)
            .map_err(|e| -> PdfBackendError { PdfBackendError::Parse(e.to_owned()) })?;

            let (month, rest) = rest.unwrap_or((1, None));
            let (day, rest) = rest.unwrap_or((1, None));
            let (hour, rest) = rest.unwrap_or((0, None));
            let (minute, rest) = rest.unwrap_or((0, None));
            let (second, rest) = rest.unwrap_or((0, None));
            let (offset_direction, rest) = rest.unwrap_or(('Z', None));
            let (offset_hour, rest) = rest.unwrap_or((0, None));
            let offset_minute = rest.unwrap_or(0);

            let utc_offset = match offset_direction {
                'Z' => UtcOffset::UTC,
                '+' => UtcOffset::from_hms(offset_hour, offset_minute, 0).inspect_err(|_| {
                    warn!("failed to construct a positive UtcOffset");
                })?,
                '-' => UtcOffset::from_hms(-offset_hour, offset_minute, 0).inspect_err(|_| {
                    warn!("failed to construct a negative UtcOffset");
                })?,
                _ => unreachable!(),
            };

            let offset_date_time = PrimitiveDateTime::new(
                Date::from_calendar_date(
                    year,
                    Month::try_from(month).inspect_err(|_| {
                        warn!("failed to constuct a Month");
                    })?,
                    day,
                )
                .inspect_err(|_| {
                    warn!("failed to construct a Date");
                })?,
                Time::from_hms(hour, minute, second).inspect_err(|_| {
                    warn!("failed to construct a Time");
                })?,
            )
            .assume_offset(utc_offset);

            Ok(offset_date_time)
        })();

        Self {
            raw: raw.to_string(),
            parsed: parsed
                .map_err(|e| warn!("failed to parse PDF date/time string {raw:?}: {e}"))
                .ok(),
        }
    }
}
