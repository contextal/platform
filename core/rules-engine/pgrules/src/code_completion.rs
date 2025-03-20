use pest::Parser;
use rules::{Rule, RuleParser};

const SYMBOLS: &[&str] = &[
    "ALL_ASCII",
    "ALL_NUMBERS",
    "CC_NUMBER",
    "CHAR_DECODING_ERRORS",
    "CHARSET_ATTM",
    "CHECKSUM_MISMATCH",
    "CODE_ALL_COMMENTS",
    "COMMENT_EXTRACTION_FAILED",
    "COMMENT_TRUNCATED",
    "CORRUPTED",
    "CORRUPTED_VBA",
    "DECOMPILED",
    "DECRYPTED",
    "DOC",
    "DOCX",
    "DUP_BCC",
    "DUP_CC",
    "DUP_ENVELOPE_TO",
    "DUP_FROM",
    "DUP_IN_REPLY_TO",
    "DUP_MESSAGE_ID",
    "DUP_REPLY_TO",
    "DUP_RETURN_PATH",
    "DUP_SUBJECT",
    "DUP_TO",
    "ENCRYPTED",
    "FALLBACK_TO_RAW_IMAGE",
    "FETCH_ERROR",
    "FETCH_INCOMPLETE",
    "FROM_LIST",
    "GZIP_MULTI_MEMBER",
    "GZIP_TRAILING_GARBAGE",
    "HAS_CHECKSUM_INCONSISTENCY",
    "HAS_FORMS",
    "HAS_MACRO_SHEET",
    "INFECTED",
    "INVALID_ATIME",
    "INVALID_BODY_ENC",
    "INVALID_CTIME",
    "INVALID_DATE",
    "INVALID_FILETIME",
    "INVALID_HEADERS",
    "INVALID_MIME_VER",
    "INVALID_MTIME",
    "INVALID_PASSWORD",
    "INVALID_SIGNING_DATE",
    "ISO9660",
    "ISSUES",
    "LIMITS_REACHED",
    "MANY_NUMBERS",
    "MAX_ANNOTATIONS_REACHED",
    "MAX_ATTACHMENTS_REACHED",
    "MAX_BOOKMARKS_REACHED",
    "MAX_FONTS_PER_PAGE_REACHED",
    "MAX_LINKS_REACHED",
    "MAX_OBJECT_DEPTH_REACHED",
    "MAX_OBJECTS_REACHED",
    "MAX_PAGES_REACHED",
    "MAX_SIGNATURES_REACHED",
    "MISSING_DATE",
    "MISSING_FROM",
    "MISSING_MESSAGE_ID",
    "MISSING_MIME_VER",
    "MISSING_SIGNING_DATE",
    "MISSING_SUBJECT",
    "MISSING_TO",
    "MOSTLY_NUMBERS",
    "NOTEXT",
    "NOT_FOUND",
    "OCR",
    "ODS",
    "ODT",
    "OLE",
    "PARTIAL_DATA",
    "RESENT",
    "RFC2397",
    "SOLID",
    "SVG_ERRORS",
    "SVG_JS",
    "TIMEOUT",
    "TOOBIG",
    "TOOSMALL",
    "TRUNCATED",
    "UDF",
    "VBA",
    "XLS",
    "XLSX",
    "ZIP_BAD",
    "ZIP_CSIZE_MISMATCH",
    "ZIP_DSIZE_MISMATCH",
    "ZIP_UNSUP",
];

const TYPES: &[&str] = &[
    "7z", "ARJ", "Bzip2", "CAB", "CDFS", "Domain", "ELF", "Email", "Gzip", "HTML", "Image", "LNK",
    "LZMA", "MSG", "MSI", "MachO", "ODF", "Office", "PDF", "PE", "RAR", "RTF", "Tar", "Text",
    "URL", "UniBin", "ZIP", "EMPTY", "SKIPPED", "UNKNOWN",
];

#[derive(Debug)]
pub enum Position {
    Byte(usize),
    Character(usize),
}

#[derive(Debug)]
pub enum Token {
    Keyword(String),
    Text(String),
}

pub fn get_code_completion(
    query: &str,
    position: Position,
    global_query: bool,
) -> Result<Vec<Token>, String> {
    let mut result = Vec::new();
    let position = match position {
        Position::Byte(p) => p,
        Position::Character(p) => calculate_byte_offset(query, p),
    };
    pest::set_error_detail(true);
    println!("{query} {position} {global_query}");
    get_code_completion_inner(query, position, global_query, &mut result);
    pest::set_error_detail(false);
    Ok(result)
}

fn calculate_byte_offset(text: &str, char_offset: usize) -> usize {
    if char_offset == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_offset)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn get_code_completion_inner(
    query: &str,
    position: usize,
    global_query: bool,
    result: &mut Vec<Token>,
) {
    if position > query.len() {
        return;
    }
    let input = &query[0..position];
    let r = if global_query {
        Rule::rule_global
    } else {
        Rule::rule
    };
    let Err(err) = RuleParser::parse(r, input) else {
        // for token in ["&&", "and", "or", "||"] {
        //     result.push(Token::Keyword(token.to_string()));
        // }
        return;
    };

    println!("ERROR: {err:#?}");

    let error_position = match err.location {
        pest::error::InputLocation::Pos(p) => p,
        pest::error::InputLocation::Span((p, _)) => p,
    };

    if error_position < position {
        let slice = &input[error_position..position];
        if slice.contains(|c: char| c.is_whitespace() || c == ')' || c == '}') {
            return;
        }
    }

    let Some(parse_attempts) = &err.parse_attempts() else {
        return;
    };
    let positives =
        if let pest::error::ErrorVariant::ParsingError { mut positives, .. } = err.variant {
            positives.retain(|r| *r != Rule::COMMENT);
            positives
        } else {
            Vec::new()
        };

    if (positives == [Rule::variable, Rule::node]
        || positives
            == [
                Rule::variable,
                Rule::gqs_matches,
                Rule::gqs_time_window,
                Rule::gqs_max_neighbors,
                Rule::node,
            ])
        && input.chars().nth(position.saturating_sub(1)) == Some(';')
    {
        return;
    }

    let mut global_settings = None;
    let mut expected_tokens = parse_attempts
        .expected_tokens()
        .iter()
        .filter_map(|t| {
            let mut token = t.to_string();
            if ["MATCHES:", "TIME_WINDOW:", "MAX_NEIGHBORS:"].contains(&token.as_str()) {
                let global_settings = match &global_settings {
                    Some(v) => v,
                    None => {
                        let v = extract_global_settings(input);
                        global_settings = Some(v);
                        global_settings.as_ref().unwrap()
                    }
                };
                let r = match token.as_str() {
                    "MATCHES:" => Rule::gqs_matches,
                    "TIME_WINDOW:" => Rule::gqs_time_window,
                    "MAX_NEIGHBORS:" => Rule::gqs_max_neighbors,
                    _ => unreachable!(),
                };
                if global_settings.contains(&r) {
                    return None;
                }
            } else if !["LOCAL", "NONE"].contains(&token.as_str()) {
                token = token.to_lowercase();
            }

            let token_trimmed = token.trim();
            let ignored = [
                "//", "/*", "+", "-", "0..9", "_", "a..f", "a..z", "\\", ".", "e",
            ];

            if token_trimmed.is_empty() || ignored.contains(&token_trimmed) {
                return None;
            }
            token.truncate(token_trimmed.bytes().len());
            Some(token)
        })
        .collect::<Vec<_>>();

    if positives.iter().any(|r| {
        [
            Rule::clam_hex_alternative,
            Rule::clam_hex_alternative_generic,
            Rule::clam_hex_alternative_multibyte,
            Rule::clam_hex_alternative_multibyte_part,
            Rule::clam_hex_alternative_singlebyte,
            Rule::clam_hex_signature,
            Rule::clam_hex_signature_alt,
            Rule::clam_hex_signature_byte,
            Rule::clam_hex_signature_byte_simple,
            Rule::clam_hex_splitter,
            Rule::clam_hex_subsignature,
            Rule::clam_hex_wildcard_repetition,
            Rule::clam_offset,
            Rule::clam_pattern,
        ]
        .contains(r)
    }) {
        expected_tokens.retain(|t| ["${", "}"].contains(&t.as_str()));
    }

    if expected_tokens == ["\""] {
        let Some(mut index) = input.rfind('"') else {
            return;
        };
        if input.as_bytes().get(index.saturating_sub(1)) == Some(&b'r') {
            index -= 1;
        }
        let input = &input[0..index];
        let Err(err) = RuleParser::parse(r, input) else {
            return;
        };
        let pest::error::ErrorVariant::ParsingError { positives, .. } = err.variant else {
            return;
        };
        if positives.contains(&Rule::string_symbol) {
            for token in SYMBOLS.iter().map(|s| Token::Text(s.to_string())) {
                result.push(token);
            }
        } else if positives.contains(&Rule::constant_string_object_type) {
            for token in TYPES.iter().map(|s| Token::Text(s.to_string())) {
                result.push(token);
            }
        }
    }

    if expected_tokens.contains(&"r\"".to_string()) {
        let mut erase_quotes = false;
        if positives.contains(&Rule::string_symbol) {
            erase_quotes = true;
            for token in SYMBOLS.iter().map(|s| Token::Text(format!(r#""{s}""#))) {
                result.push(token);
            }
        } else if positives.contains(&Rule::constant_string_object_type) {
            erase_quotes = true;
            for token in TYPES.iter().map(|s| Token::Text(format!(r#""{s}""#))) {
                result.push(token);
            }
        }
        if erase_quotes {
            expected_tokens.retain(|t| t != "\"" && t != "r\"");
        }
    }
    if expected_tokens.is_empty()
        || expected_tokens == ["}"]
        || expected_tokens.contains(&"${".to_string())
    {
        let variables = [
            Rule::variable_bool,
            Rule::variable_clam_pattern,
            Rule::variable_date,
            Rule::variable_json,
            Rule::variable_number,
            Rule::variable_selector,
            Rule::variable_string,
        ];
        if let Some(r) = positives.iter().find(|r| variables.contains(r)) {
            expected_tokens.retain(|t| t != "${");
            let mut variables = extract_variables(input, *r, global_query);
            variables.sort();
            variables.dedup();
            for token in variables {
                result.push(Token::Keyword(token));
            }
        }
    }
    expected_tokens.sort();
    expected_tokens.dedup();
    for token in expected_tokens {
        result.push(Token::Keyword(token));
    }
}

fn extract_variables(input: &str, variable_type: Rule, global_query: bool) -> Vec<String> {
    let mut result = Vec::new();
    let r = if global_query {
        Rule::rule_body_partial_global
    } else {
        Rule::rule_variables
    };
    let Ok(mut partial_pairs) = RuleParser::parse(r, input) else {
        return result;
    };
    let partial_pairs = {
        //safe
        let mut r = partial_pairs.next().unwrap().into_inner();
        if let Some(pair) = r.peek() {
            if pair.as_rule() == Rule::global_query_settings {
                //safe
                r = r.nth(1).unwrap().into_inner();
            }
        }
        r
    };
    let compatible_types: &[Rule] = match variable_type {
        Rule::variable_bool => &[Rule::variable_value_bool],
        Rule::variable_clam_pattern => &[Rule::variable_value_clam_pattern],
        Rule::variable_date => &[Rule::variable_value_date],
        Rule::variable_json => &[
            Rule::variable_value_bool,
            Rule::variable_value_number,
            Rule::variable_value_string,
        ],
        Rule::variable_number => &[Rule::variable_value_number],
        Rule::variable_selector => &[Rule::variable_value_selector],
        Rule::variable_string => &[Rule::variable_value_string],
        _ => unreachable!(),
    };
    for pair in partial_pairs {
        let mut inner = pair.into_inner();
        //safe
        let name_pair = inner.next().unwrap();
        let name = name_pair.as_str();
        //safe
        let value_pair = inner.next().unwrap().into_inner().next().unwrap();
        let value_rule = value_pair.as_rule();
        if compatible_types.contains(&value_rule) {
            result.push(name.to_string());
        }
    }
    result
}

// #[test]
// fn test_get_completion() {
//     let query = r#"true && size == @match_pattern("deadbeef")"#;
//     assert_eq!(get_code_completion(query, 4), ["&&", "and", "or", "||"]);
//     assert_eq!(get_code_completion(query, 6), ["&&", "and", "or", "||"]);
//     assert_eq!(get_code_completion(query, 9), [""]);
// }

fn extract_global_settings(input: &str) -> Vec<Rule> {
    let mut result = Vec::new();
    let Ok(mut partial_pairs) = RuleParser::parse(Rule::global_query_settings, input) else {
        return result;
    };
    let partial_pairs = partial_pairs.next().unwrap().into_inner();
    for r in partial_pairs {
        result.push(r.as_rule());
    }
    result
}
