use pest::Parser;
use rules::{Rule, RuleParser};
use wasm_bindgen::prelude::*;

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
    expected: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct QLResult {
    error: Option<QLError>,
    tokens: Vec<QLToken>,
}

impl QLResult {
    fn new(query: &str) -> Self {
        let mut res = Self {
            error: None,
            tokens: Vec::new(),
        };
        match RuleParser::parse(Rule::rule, query) {
            Ok(pairs) => {
                Self::collect_pairs(pairs, &mut res.tokens, &mut res.error);
            }
            Err(e) => {
                let mut error = QLError {
                    position: match e.location {
                        pest::error::InputLocation::Pos(p) => p,
                        _ => 0,
                    },
                    size: None,
                    expected: Vec::new(),
                };
                if let pest::error::ErrorVariant::ParsingError { positives, .. } = e.variant {
                    error
                        .expected
                        .extend(positives.into_iter().map(|s| format!("{s:?}")));
                }
                res.error = Some(error);
                if let Ok(partial_pairs) = RuleParser::parse(Rule::node, query) {
                    Self::collect_pairs(partial_pairs, &mut res.tokens, &mut res.error);
                }
            }
        }
        res
    }

    fn collect_pairs(
        pairs: pest::iterators::Pairs<Rule>,
        tokens: &mut Vec<QLToken>,
        error: &mut Option<QLError>,
    ) {
        for p in pairs {
            if error.is_some() {
                return;
            }

            let name = match p.as_rule() {
                Rule::functions_bool | Rule::functions_number | Rule::functions_string => {
                    "functions".to_string()
                }
                Rule::ident_bool | Rule::ident_number | Rule::ident_string => "ident".to_string(),
                Rule::string_raw | Rule::string_regular => "string".to_string(),
                _ => format!("{:?}", p.as_rule()),
            };
            tokens.push(QLToken {
                name: name.to_string(),
                start: p.as_span().start(),
                end: p.as_span().end(),
            });
            if name == "string" && rules::unescape_string(p.clone()).is_err() {
                let size = p.as_span().end().saturating_sub(p.as_span().start());
                let size = if size == 0 { None } else { Some(size) };
                *error = Some(QLError {
                    position: p.as_span().start(),
                    size,
                    expected: vec!["string".to_string()],
                });
                return;
            }

            if p.as_rule() != Rule::string {
                Self::collect_pairs(p.into_inner(), tokens, error);
            }
        }
    }
}

#[wasm_bindgen]
pub fn ql_check(rule_str: &str) -> JsValue {
    let res = QLResult::new(rule_str);
    serde_wasm_bindgen::to_value(&res).unwrap()
}

#[wasm_bindgen]
pub fn ql_to_sqlite(rule_str: &str) -> String {
    pgrules::parse_to_sql(rule_str).unwrap()
}

#[wasm_bindgen]
pub fn ql_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
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
        );
        assert_eq!(res.error, None);
        let res = QLResult::new("size == size");
        assert_eq!(res.error, None);
        let res = QLResult::new("size \"string\"");
        assert_eq!(res.error.unwrap().position, 5);
    }
}
