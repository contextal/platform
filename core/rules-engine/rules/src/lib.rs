#[derive(pest_derive::Parser)]
#[grammar = "rules.pest"]
pub struct RuleParser;

pub fn unescape_string(
    pair: pest::iterators::Pair<Rule>,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let inner = match pair.as_rule() {
        Rule::string => {
            let inner = pair
                .into_inner()
                .next()
                .expect("Expecting string_regular or string_raw!");
            inner
        }
        Rule::string_raw | Rule::string_regular => pair,
        _ => {
            unreachable!(
                "Invalid rule {:?}. Expecting on of {:?}",
                pair.as_rule(),
                [Rule::string, Rule::string_raw, Rule::string_regular]
            );
        }
    };
    let is_raw_string = match inner.as_rule() {
        Rule::string_regular => false,
        Rule::string_raw => true,
        _ => unreachable!("Expecting string_regular or string_raw!"),
    };
    let value = inner.into_inner().next().unwrap();
    if is_raw_string || !value.as_str().contains('\\') {
        Ok(value.as_str().to_string())
    } else {
        let mut result = String::with_capacity(value.as_str().len());
        let mut iter = value.as_str().chars();
        while let Some(c) = iter.next() {
            if c != '\\' {
                result.push(c);
                continue;
            }
            match iter.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('u') => {
                    let hex_string = String::from_iter([
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                    ]);
                    let code = u32::from_str_radix(&hex_string, 16)
                        .expect("Invalid hexadecimal unicode format");
                    let c = char::from_u32(code).ok_or_else(|| {
                        pest::error::Error::new_from_span(
                            // This error will be printed in UI as Expected ...
                            pest::error::ErrorVariant::CustomError {
                                message: "Valid character code".to_string(),
                            },
                            value.as_span(),
                        )
                    })?;
                    result.push(c);
                }
                Some('U') => {
                    let hex_string = String::from_iter([
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                        iter.next().expect("Unexpected EOF"),
                    ]);
                    let code = u32::from_str_radix(&hex_string, 16)
                        .expect("Invalid hexadecimal unicode format");
                    let c = char::from_u32(code).ok_or_else(|| {
                        pest::error::Error::new_from_span(
                            // This error will be printed in UI as Expected ...
                            pest::error::ErrorVariant::CustomError {
                                message: "Valid character code".to_string(),
                            },
                            value.as_span(),
                        )
                    })?;
                    result.push(c);
                }
                _ => unreachable!("Unsupported escape character"),
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
fn parse_rule(input: &str) -> String {
    use pest::Parser;
    let mut parsed = RuleParser::parse(Rule::string, input).unwrap();
    let parsed = parsed.next().unwrap();
    unescape_string(parsed).unwrap()
}

#[test]
fn test_parse_string() {
    assert_eq!(parse_rule(r#""""#), "");
    assert_eq!(parse_rule(r#""text""#), "text");
    assert_eq!(parse_rule(r#""\"\r\n\t\u0030""#), "\"\r\n\t0");
    assert_eq!(parse_rule(r#"r"""#), "");
    assert_eq!(parse_rule(r#"r"text""#), "text");
    assert_eq!(parse_rule(r#"r"\r\n\t\u0030""#), "\\r\\n\\t\\u0030");
}
