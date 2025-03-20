use pest::Parser;
use pgrules::{PairWrapper, QueryType};
use rules::{Rule, RuleParser};
use semver::VersionReq;
use wasm_bindgen::prelude::*;

// #[wasm_bindgen(start)]
// pub fn start() -> Result<(), JsValue> {
//     tracing_wasm::set_as_global_default();
//     Ok(())
// }

#[derive(Debug, serde::Serialize)]
struct QLToken {
    name: String,
    start: usize,
    end: usize,
}

#[derive(Debug, serde::Serialize, PartialEq)]
struct QLError {
    position: usize,
    size: Option<usize>,
    message: Option<String>,
    expected: Vec<String>,
}

impl QLError {
    fn from_pest_error(query: &str, mut error: pest::error::Error<Rule>) -> Self {
        let mut change_to_custom_error = false;
        match &mut error.variant {
            pest::error::ErrorVariant::ParsingError {
                positives,
                negatives,
            } => {
                positives.retain(|v| *v != Rule::COMMENT);
                if positives.is_empty() && negatives.is_empty() {
                    change_to_custom_error = true;
                }
            }
            pest::error::ErrorVariant::CustomError { .. } => {}
        };
        if change_to_custom_error {
            error.variant = pest::error::ErrorVariant::CustomError {
                message: "comment, function or complete rule".to_string(),
            }
        }
        let (position, size) = match error.location {
            pest::error::InputLocation::Pos(p) => {
                let start = get_character_position(query, p);
                (start, None)
            }
            pest::error::InputLocation::Span((p, s)) => {
                let start = get_character_position(query, p);
                let end = get_character_position(query, s);
                (start, end.checked_sub(start))
            }
        };
        let mut result = QLError {
            position,
            size,
            expected: Vec::new(),
            message: None,
        };

        match error.variant {
            pest::error::ErrorVariant::ParsingError { positives, .. } => {
                result
                    .expected
                    .extend(positives.into_iter().map(|s| format!("{s:?}")));
            }
            pest::error::ErrorVariant::CustomError { message } => result.message = Some(message),
        }
        result
    }
}

#[derive(Debug, serde::Serialize)]
struct QLResult {
    error: Option<QLError>,
    tokens: Vec<QLToken>,
}

impl QLResult {
    fn new(query: &str, context: QueryType) -> Self {
        let mut res = Self {
            error: None,
            tokens: Vec::new(),
        };
        let (r, partial_rule) = match context {
            QueryType::ScenarioGlobal => (Rule::rule_global, Rule::rule_body_partial_global),
            _ => (Rule::rule, Rule::rule_body_partial),
        };
        match RuleParser::parse(r, query) {
            Ok(pairs) => {
                let parsed = pairs.clone().next().unwrap(); // cannot fail: rule matches from SOI to EOI
                if let Err(e) = pgrules::to_sql(PairWrapper(parsed), 0, context) {
                    let error = QLError::from_pest_error(query, *e);
                    res.error = Some(error);
                }
                res.collect_pairs(query, pairs);
            }
            Err(e) => {
                let error = QLError::from_pest_error(query, e);
                if let Ok(partial_pairs) = RuleParser::parse(partial_rule, query) {
                    res.collect_pairs(query, partial_pairs);
                }
                res.error = Some(error);
            }
        }
        res
    }

    fn collect_pairs(&mut self, query: &str, pairs: pest::iterators::Pairs<Rule>) {
        for p in pairs {
            if self.error.is_some() {
                return;
            }
            let mut collect_inner = true;
            let name = match p.as_rule() {
                Rule::functions_bool | Rule::functions_number | Rule::functions_string => {
                    "functions".to_string()
                }
                Rule::ident_bool
                | Rule::ident_number
                | Rule::ident_string
                | Rule::ident_string_object_type => "ident".to_string(),
                Rule::string_raw | Rule::string_regular => "string".to_string(),
                Rule::variable_bool
                | Rule::variable_clam_pattern
                | Rule::variable_date
                | Rule::variable_json
                | Rule::variable_number
                | Rule::variable_selector
                | Rule::variable_string => "variable".to_string(),
                Rule::rule
                | Rule::rule_body
                | Rule::rule_body_partial
                | Rule::rule_variables
                | Rule::node
                | Rule::cond
                | Rule::variable_definition
                | Rule::variable_value
                | Rule::variable_value_bool
                | Rule::variable_value_clam_pattern
                | Rule::variable_value_date
                | Rule::variable_value_number
                | Rule::variable_value_selector_filter
                | Rule::variable_value_selector_get
                | Rule::rule_variables_global
                | Rule::variable_definition_global
                | Rule::variable_value_global
                | Rule::EOI => String::new(),
                Rule::gqs_matches | Rule::gqs_time_window | Rule::gqs_max_neighbors => {
                    "setting_key".to_string()
                }
                Rule::gqs_matches_value
                | Rule::gqs_time_window_value
                | Rule::gqs_max_neighbors_value => {
                    collect_inner = false;
                    "setting_value".to_string()
                }
                Rule::unsigned_integer => "number".to_string(),
                _ => format!("{:?}", p.as_rule()),
            };

            if !name.is_empty() {
                self.tokens.push(QLToken {
                    name: name.to_string(),
                    start: get_character_position(query, p.as_span().start()),
                    end: get_character_position(query, p.as_span().end()),
                });
            }

            if name == "string" && rules::unescape_string(p.clone()).is_err() {
                let size = p.as_span().end().saturating_sub(p.as_span().start());
                let size = if size == 0 { None } else { Some(size) };
                self.error = Some(QLError {
                    position: p.as_span().start(),
                    size,
                    expected: vec!["string".to_string()],
                    message: None,
                });
                return;
            }

            if collect_inner && p.as_rule() != Rule::string {
                self.collect_pairs(query, p.into_inner());
            }
        }
    }
}

#[wasm_bindgen]
pub fn ql_check(rule_str: &str, is_global_query: bool) -> JsValue {
    let context = if is_global_query {
        QueryType::ScenarioGlobal
    } else {
        QueryType::ScenarioLocal
    };
    let res = QLResult::new(rule_str, context);
    serde_wasm_bindgen::to_value(&res).unwrap()
}

#[wasm_bindgen]
pub fn ql_detect_scenario_version(local_rule: &str, global_rule: &str) -> String {
    let local_rule = local_rule.trim();
    let global_rule = global_rule.trim();
    if local_rule.is_empty() {
        return String::new();
    }
    let mut version = pgrules::detect_query_version(local_rule).unwrap();
    if !global_rule.is_empty() {
        let global_version = pgrules::detect_query_version(global_rule).unwrap();
        version = version.max(global_version);
    }
    version.to_comparator().to_string()
}

#[wasm_bindgen]
pub fn ql_to_sql(rule_str: &str) -> String {
    pgrules::parse_to_sql(rule_str, QueryType::Search)
        .unwrap()
        .query
}

#[derive(Debug, serde::Serialize)]
struct Completion {
    label: String,
    #[serde(rename = "type")]
    completion_type: String,
}

#[derive(Debug, serde::Serialize)]
struct CompletionResult {
    completion: Vec<Completion>,
    error: Option<String>,
}

#[wasm_bindgen]
pub fn ql_get_code_completion(rule_str: &str, position: usize, global_query: bool) -> JsValue {
    let tokens = pgrules::get_code_completion(
        rule_str,
        pgrules::Position::Character(position),
        global_query,
    );
    let result = match tokens {
        Ok(tokens) => CompletionResult {
            completion: tokens
                .into_iter()
                .map(|t| match t {
                    pgrules::Token::Keyword(label) => Completion {
                        label,
                        completion_type: "keyword".to_string(),
                    },
                    pgrules::Token::Text(label) => Completion {
                        label,
                        completion_type: "text".to_string(),
                    },
                })
                .collect(),
            error: None,
        },
        Err(error) => CompletionResult {
            completion: Vec::new(),
            error: Some(error),
        },
    };
    serde_wasm_bindgen::to_value(&result).unwrap()
}

#[wasm_bindgen]
pub fn ql_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(Debug, serde::Serialize)]
struct VerificationResult {
    result: bool,
    error: Option<String>,
}

#[wasm_bindgen]
pub fn verify_compatible_with(input: &str) -> JsValue {
    let result = if let Err(error) = VersionReq::parse(input) {
        VerificationResult {
            result: false,
            error: Some(error.to_string()),
        }
    } else {
        VerificationResult {
            result: true,
            error: None,
        }
    };
    serde_wasm_bindgen::to_value(&result).unwrap()
}

// Map byte position to closest character position
fn get_character_position(text: &str, byte_position: usize) -> usize {
    let mut result = 0;
    for (byte_index, c) in text.char_indices() {
        if byte_index + c.len_utf8() > byte_position {
            break;
        }
        result += 1;
    }
    result
}

#[test]
fn test_get_character_position() {
    let text = "abc";
    assert_eq!(get_character_position(text, 0), 0);
    assert_eq!(get_character_position(text, 1), 1);
    assert_eq!(get_character_position(text, 2), 2);
    assert_eq!(get_character_position(text, 3), 3);
    let text = "żółć";
    assert_eq!(text.len(), 8);
    assert_eq!(get_character_position(text, 0), 0);
    assert_eq!(get_character_position(text, 1), 0);
    assert_eq!(get_character_position(text, 2), 1);
    assert_eq!(get_character_position(text, 3), 1);
    assert_eq!(get_character_position(text, 4), 2);
    assert_eq!(get_character_position(text, 5), 2);
    assert_eq!(get_character_position(text, 6), 3);
    assert_eq!(get_character_position(text, 7), 3);
    assert_eq!(get_character_position(text, 8), 4);
}

#[cfg(test)]
mod tests {
    use pgrules::parse_to_sql;

    use super::*;

    #[test]
    fn test_query() {
        let res = QLResult::new(
            r#"is_entry &&
             (object_type = "Email" or object_type == "asd") and
             ! @has_symbol("DONT_WANT") ||
             @has_symbol("DO_WANT") == true and
             (
               size > 42
               or
               @match_object_meta($headers regex("HTTP")) != true
             )
            "#,
            QueryType::Search,
        );
        assert_eq!(res.error, None);
        let res = QLResult::new("size == size", QueryType::Search);
        assert_eq!(res.error, None);
        let res = QLResult::new("size \"string\"", QueryType::Search);
        assert_eq!(res.error.unwrap().position, 5);
        let res = QLResult::new("@match_pattern(aa)", QueryType::Search);
        let error = res.error.unwrap();
        assert_eq!(error.position, 15);
        assert_eq!(error.size, Some(2));
        assert_eq!(
            error.message,
            Some("Sub-signature containing a block of two static bytes".to_string())
        );
        assert_eq!(error.expected.len(), 0);
        let input = "size/*comment1*/==/*comment2*/1//comment3";
        let ql = parse_to_sql(input, QueryType::Search).unwrap().query;
        assert_eq!(
            ql,
            r#"FROM objects AS "objects_0" WHERE ("objects_0"."size"=1)"#
        );
        let res = QLResult::new(input, QueryType::Search);
        assert_eq!(
            res.tokens
                .iter()
                .map(|t| t.name.to_string())
                .collect::<Vec<_>>(),
            ["ident", "COMMENT", "op", "equals", "COMMENT", "number", "COMMENT",]
        );
        let res = QLResult::new("${x}=1;size==${x}", QueryType::Search);
        assert_eq!(
            res.tokens
                .iter()
                .map(|t| t.name.to_string())
                .collect::<Vec<_>>(),
            ["variable", "number", "ident", "op", "equals", "variable",]
        );
        let res = QLResult::new("${x}=1;size==", QueryType::Search);
        assert_eq!(
            res.tokens
                .iter()
                .map(|t| t.name.to_string())
                .collect::<Vec<_>>(),
            ["variable", "number"]
        );
    }
}
