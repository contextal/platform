use pest::{iterators::Pairs, Token};
use rules::Rule;
use semver::{BuildMetadata, Comparator, Op, Prerelease, Version};

pub const CURRENT_VERSION_STR: &str = "1.3.0";
pub const CURRENT_VERSION: Version = parse_version(CURRENT_VERSION_STR);

const fn parse_version(version: &str) -> Version {
    let bytes = version.as_bytes();
    let mut numbers = [0_u64; 3];
    let mut number_index = 0;
    let mut byte_index = 0;
    let mut at_start = true;

    while byte_index < bytes.len() {
        let c = bytes[byte_index];
        byte_index += 1;
        if c == b'.' {
            number_index += 1;
            if at_start || byte_index == bytes.len() || number_index >= numbers.len() {
                panic!("Invalid CURRENT_VERSION");
            }
            at_start = true;
            continue;
        }
        let lower_bound = if !at_start || byte_index == bytes.len() || bytes[byte_index] == b'.' {
            b'0'
        } else {
            b'1'
        };
        at_start = false;
        if c < lower_bound || c > b'9' {
            panic!("Invalid CURRENT_VERSION");
        }
        let v = (c - b'0') as u64;
        let current = &mut numbers[number_index];
        *current *= 10;
        *current += v;
    }
    if number_index != numbers.len() - 1 {
        panic!("Invalid CURRENT_VERSION");
    }
    Version {
        major: numbers[0],
        minor: numbers[1],
        patch: numbers[2],
        pre: Prerelease::EMPTY,
        build: BuildMetadata::EMPTY,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl RuleVersion {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
    pub fn to_comparator(&self) -> Comparator {
        let major = self.major;
        let (minor, patch) = if self.minor == 0 && self.patch == 0 {
            (None, None)
        } else if self.patch == 0 {
            (Some(self.minor), None)
        } else {
            (Some(self.minor), Some(self.patch))
        };
        Comparator {
            op: Op::GreaterEq,
            major,
            minor,
            patch,
            pre: Prerelease::EMPTY,
        }
    }
    pub(crate) fn from_pairs(pairs: Pairs<Rule>) -> Self {
        let mut result = RuleVersion::new(1, 0, 0);
        let tokens = pairs.into_iter().flatten().tokens().collect::<Vec<_>>();
        for token in tokens {
            let Token::Start { rule: r, .. } = token else {
                continue;
            };
            let version = Self::from_rule(r);
            result = version.max(result);
        }
        result
    }
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
            | Rule::constant_string_object_type
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
            | Rule::ident_string_object_type
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
            | Rule::unsigned_integer
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
            | Rule::rule_body
            | Rule::rule_body_partial
            | Rule::rule_variables
            | Rule::string
            | Rule::string_raw
            | Rule::string_raw_value
            | Rule::string_regular
            | Rule::string_regular_value
            | Rule::string_symbol
            | Rule::time
            | Rule::WHITESPACE
            | Rule::variable_value_bool
            | Rule::variable_value_clam_pattern
            | Rule::variable_value_date
            | Rule::variable_definition
            | Rule::variable_value_number
            | Rule::variable_value_string
            | Rule::variable_value
            | Rule::jsonpath_object_match
            | Rule::jsonpath_object_match_condition
            | Rule::jsonpath_object_match_id
            | Rule::jsonpath_object_match_condition_simple
            | Rule::jsonpath_object_match_condition_node
            | Rule::jsonpath_object_match_equals
            | Rule::jsonpath_object_match_compares
            | Rule::count_conditions_fn
            | Rule::in_operator
            | Rule::in_statement_number
            | Rule::in_statement_string
            | Rule::in_statement_string_object_type
            | Rule::in_statement_string_extended_entry
            | Rule::in_statement_string_extended
            | Rule::in_statement_string_symbol
            | Rule::in_statement_string_symbol_entry
            | Rule::in_statement_jsonpath_entry
            | Rule::in_statement_jsonpath
            | Rule::in_statement_jsonpath_object
            | Rule::in_statement_jsonpath_object_entry
            | Rule::variable_value_selector
            | Rule::variable_value_selector_filter
            | Rule::variable_value_selector_get
            | Rule::get_symbols_fn
            | Rule::get_names_fn
            | Rule::get_object_meta_fn
            | Rule::get_relation_meta_fn
            | Rule::in_statement_selector
            | Rule::variable_bool
            | Rule::variable_number
            | Rule::variable_string
            | Rule::variable_date
            | Rule::variable_clam_pattern
            | Rule::variable_json
            | Rule::variable_selector
            | Rule::variable_value_global
            | Rule::variable_definition_global
            | Rule::rule_variables_global
            | Rule::rule_body_global
            | Rule::rule_body_partial_global
            | Rule::rule_global
            | Rule::global_query_settings
            | Rule::global_query_setting
            | Rule::gqs_matches
            | Rule::gqs_matches_value
            | Rule::gqs_time_window
            | Rule::gqs_time_window_value
            | Rule::gqs_time_window_unit
            | Rule::gqs_max_neighbors
            | Rule::gqs_max_neighbors_value
            | Rule::variable => RuleVersion::new(1, 3, 0),
        }
    }
}

#[test]
fn test_from_pairs() {
    use pest::Parser;
    use rules::RuleParser;
    let query = r#"size=1"#;
    let parsed = RuleParser::parse(Rule::rule, query).unwrap();
    let version = RuleVersion::from_pairs(parsed);
    assert_eq!(version, RuleVersion::new(1, 3, 0));
    let comparator = version.to_comparator();
    assert_eq!(comparator.to_string(), ">=1.3");
    let query = r#"is_entry && size in (1,2,3)"#;
    let parsed = RuleParser::parse(Rule::rule, query).unwrap();
    let version = RuleVersion::from_pairs(parsed);
    assert_eq!(version, RuleVersion::new(1, 3, 0));
    let comparator = version.to_comparator();
    assert_eq!(comparator.to_string(), ">=1.3");
}
