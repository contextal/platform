use pest::Parser;
use pgrules::PairWrapper;
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

impl QLError {
    fn from_pest_error(error: pest::error::Error<Rule>) -> Self {
        let (position, size) = match error.location {
            pest::error::InputLocation::Pos(p) => (p, None),
            pest::error::InputLocation::Span((p, s)) => (p, s.checked_sub(p)),
        };
        let mut result = QLError {
            position,
            size,
            expected: Vec::new(),
        };

        match error.variant {
            pest::error::ErrorVariant::ParsingError { positives, .. } => {
                result
                    .expected
                    .extend(positives.into_iter().map(|s| format!("{s:?}")));
            }
            pest::error::ErrorVariant::CustomError { message } => {
                result.expected.push(message);
            }
        }
        result
    }
}

#[derive(Debug, serde::Serialize)]
struct QLResult {
    error: Option<QLError>,
    tokens: Vec<QLToken>,
    min_version: MinVersion,
}

impl QLResult {
    fn new(query: &str) -> Self {
        let mut res = Self {
            error: None,
            tokens: Vec::new(),
            min_version: MinVersion::V1_0,
        };
        match RuleParser::parse(Rule::rule, query) {
            Ok(pairs) => {
                let parsed = pairs.clone().next().unwrap(); // cannot fail: rule matches from SOI to EOI
                if let Err(e) = pgrules::to_sql(PairWrapper(parsed), 0, false) {
                    let error = QLError::from_pest_error(*e);
                    res.error = Some(error);
                }
                res.collect_pairs(pairs);
            }
            Err(e) => {
                let error = QLError::from_pest_error(e);
                res.error = Some(error);
                if let Ok(partial_pairs) = RuleParser::parse(Rule::node, query) {
                    res.collect_pairs(partial_pairs);
                }
            }
        }
        res
    }

    fn collect_pairs(&mut self, pairs: pest::iterators::Pairs<Rule>) {
        for p in pairs {
            if self.error.is_some() {
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
            let version = MinVersion::from_rule(p.as_rule());
            if version > self.min_version {
                self.min_version = version;
            }

            self.tokens.push(QLToken {
                name: name.to_string(),
                start: p.as_span().start(),
                end: p.as_span().end(),
            });
            if name == "string" && rules::unescape_string(p.clone()).is_err() {
                let size = p.as_span().end().saturating_sub(p.as_span().start());
                let size = if size == 0 { None } else { Some(size) };
                self.error = Some(QLError {
                    position: p.as_span().start(),
                    size,
                    expected: vec!["string".to_string()],
                });
                return;
            }

            if p.as_rule() != Rule::string {
                self.collect_pairs(p.into_inner());
            }
        }
    }
}

#[derive(Debug, PartialEq, PartialOrd, serde::Serialize)]
enum MinVersion {
    #[serde(rename(serialize = "v.1"))]
    V1_0,
    #[serde(rename(serialize = "v.1.2"))]
    V1_2,
}

impl MinVersion {
    fn from_rule(r: Rule) -> Self {
        match r {
            Rule::bool
            | Rule::char
            | Rule::clam_hex_alternative
            | Rule::clam_hex_alternative_generic
            | Rule::clam_hex_alternative_multibyte
            | Rule::clam_hex_alternative_multibyte_part
            | Rule::clam_hex_alternative_singlebyte
            | Rule::clam_hex_signature
            | Rule::clam_hex_signature_alt
            | Rule::clam_hex_signature_byte
            | Rule::clam_hex_signature_byte_simple
            | Rule::clam_hex_splitter
            | Rule::clam_hex_subsignature
            | Rule::clam_hex_wildcard_repetition
            | Rule::clam_offset
            | Rule::clam_pattern
            | Rule::COMMENT
            | Rule::compares
            | Rule::cond
            | Rule::constant_string
            | Rule::count_ancestors_fn
            | Rule::count_children_fn
            | Rule::count_descendants_fn
            | Rule::count_siblings_fn
            | Rule::date
            | Rule::date_range_fn
            | Rule::date_since_fn
            | Rule::date_string
            | Rule::datetime
            | Rule::EOI
            | Rule::equals
            | Rule::func_arg_iregex
            | Rule::func_arg_regex
            | Rule::func_arg_starts_with
            | Rule::functions
            | Rule::functions_bool
            | Rule::functions_number
            | Rule::functions_string
            | Rule::get_hash_fn
            | Rule::glue
            | Rule::has_ancestor_fn
            | Rule::has_child_fn
            | Rule::has_descendant_fn
            | Rule::has_error_fn
            | Rule::has_name_fn
            | Rule::has_object_meta_fn
            | Rule::has_parent_fn
            | Rule::has_relation_meta_fn
            | Rule::has_root_fn
            | Rule::has_sibling_fn
            | Rule::has_symbol_fn
            | Rule::ident_bool
            | Rule::ident_number
            | Rule::ident_string
            | Rule::integer
            | Rule::is_leaf_fn
            | Rule::is_root_fn
            | Rule::jsonpath_equals
            | Rule::jsonpath_ident
            | Rule::jsonpath_match_length
            | Rule::jsonpath_match_simple
            | Rule::jsonpath_match_simple_compares
            | Rule::jsonpath_match_simple_equals
            | Rule::jsonpath_path_simple
            | Rule::jsonpath_selector_identifier
            | Rule::jsonpath_selector_index
            | Rule::jsonpath_unsigned
            | Rule::logic_and
            | Rule::logic_not
            | Rule::logic_or
            | Rule::match_object_meta_fn
            | Rule::match_pattern_fn
            | Rule::match_relation_meta_fn
            | Rule::node
            | Rule::node_primary
            | Rule::number
            | Rule::op
            | Rule::rule
            | Rule::string
            | Rule::string_raw
            | Rule::string_raw_value
            | Rule::string_regular
            | Rule::string_regular_value
            | Rule::time
            | Rule::WHITESPACE => MinVersion::V1_0,
            Rule::variable
            | Rule::variable_bool
            | Rule::variable_clam_pattern
            | Rule::variable_date
            | Rule::variable_definition
            | Rule::variable_number
            | Rule::variable_string
            | Rule::variable_value
            | Rule::jsonpath_object_match
            | Rule::jsonpath_object_match_condition
            | Rule::jsonpath_object_match_id
            | Rule::jsonpath_object_match_condition_simple
            | Rule::jsonpath_object_match_condition_node
            | Rule::jsonpath_object_match_equals
            | Rule::jsonpath_object_match_compares => MinVersion::V1_2,
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
        );
        assert_eq!(res.min_version, MinVersion::V1_0);
        assert_eq!(res.error, None);
        let res = QLResult::new("size == size");
        assert_eq!(res.min_version, MinVersion::V1_0);
        assert_eq!(res.error, None);
        let res = QLResult::new("size \"string\"");
        assert_eq!(res.min_version, MinVersion::V1_0);
        assert_eq!(res.error.unwrap().position, 5);
        let res = QLResult::new("@match_pattern(aa)");
        assert_eq!(res.min_version, MinVersion::V1_0);
        let error = res.error.unwrap();
        assert_eq!(error.position, 15);
        assert_eq!(error.size, Some(2));
        assert_eq!(error.expected.len(), 1);
        assert_eq!(
            error.expected[0],
            "Sub-signature containing a block of two static bytes"
        );
        let input = "size/*comment1*/==/*comment2*/1//comment3";
        let ql = parse_to_sql(input).unwrap();
        assert_eq!(
            ql,
            r#"FROM objects AS "objects_0" WHERE ("objects_0"."size"=1)"#
        );
        let res = QLResult::new(input);
        assert_eq!(res.min_version, MinVersion::V1_0);
        assert_eq!(
            res.tokens
                .iter()
                .map(|t| t.name.to_string())
                .collect::<Vec<_>>(),
            [
                "rule", "node", "cond", "ident", "COMMENT", "op", "equals", "COMMENT", "number",
                "COMMENT", "EOI",
            ]
        );
        let res = QLResult::new("${x}=1;size==${x}");
        assert_eq!(res.min_version, MinVersion::V1_2);
        assert_eq!(
            res.tokens
                .iter()
                .map(|t| t.name.to_string())
                .collect::<Vec<_>>(),
            [
                "rule",
                "variable_definition",
                "variable",
                "variable_value",
                "variable_number",
                "number",
                "node",
                "cond",
                "ident",
                "op",
                "equals",
                "variable",
                "EOI"
            ]
        );
    }
}
