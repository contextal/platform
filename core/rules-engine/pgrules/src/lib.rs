use pest::{iterators::Pair, Parser, Span};
use rules::{Rule, RuleParser};
use tracing::{debug, trace};

pub struct PgRule(RuleParser);

pub struct PairWrapper<'a>(pub pest::iterators::Pair<'a, Rule>);

impl<'a> PairWrapper<'a> {
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
    fn as_rule(&self) -> Rule {
        self.0.as_rule()
    }
    fn as_span(&self) -> Span {
        self.0.as_span()
    }
    fn into_inner(self) -> PairsWrapper<'a> {
        let i = self.0.into_inner();
        PairsWrapper(i)
    }
}

impl Debug for PairWrapper<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub struct PairsWrapper<'a>(pest::iterators::Pairs<'a, Rule>);

impl<'a> Iterator for PairsWrapper<'a> {
    type Item = PairWrapper<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        for n in self.0.by_ref() {
            if n.as_rule() == Rule::COMMENT {
                continue;
            }
            return Some(PairWrapper(n));
        }
        None
    }
}

pub fn to_sql(
    pair: PairWrapper,
    rec: u32,
    single_workid: bool,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let mut res = String::new();
    trace!("Parsing: {}", pair.as_str());
    trace!("Result: {pair:#?}\n\n");
    let curobj = postgres_protocol::escape::escape_identifier(&format!("objects_{rec}"));
    let nextobj = postgres_protocol::escape::escape_identifier(&format!("objects_{}", rec + 1));
    let match_work = if single_workid {
        format!("{nextobj}.work_id = $1")
    } else {
        format!("{nextobj}.work_id = {curobj}.work_id")
    };
    match pair.as_rule() {
        Rule::rule => {
            if single_workid {
                res = format!("FROM objects AS {0} WHERE {0}.work_id = $1 AND ", curobj);
            } else {
                res = format!("FROM objects AS {curobj} WHERE ");
            }
            for p in pair.into_inner() {
                res += &to_sql(p, rec, single_workid)?;
            }
        }
        Rule::string
        | Rule::string_raw
        | Rule::string_regular
        | Rule::string_raw_value
        | Rule::string_regular_value => {
            unreachable!("Invalid usage of Rule::string*. Use rules::unescape_string and format/escape string properly to context.");
        }
        Rule::constant_string => {
            let string = escape_as_constant_string(pair.into_inner().next().unwrap().0)?;
            res += &string;
        }
        Rule::functions
        | Rule::functions_bool
        | Rule::functions_number
        | Rule::functions_string
        | Rule::op
        | Rule::cond
        | Rule::glue => {
            for p in pair.into_inner() {
                res += &to_sql(p, rec, single_workid)?;
            }
        }
        Rule::node => {
            res += "(";
            for p in pair.into_inner() {
                res += &to_sql(p, rec, single_workid)?;
            }
            res += ")";
        }
        Rule::ident_bool | Rule::is_root_fn | Rule::ident_number | Rule::ident_string => {
            let ident = match pair.as_str() {
                "is_root()" => "is_entry",
                other => other,
            };
            res += &format!(
                "{curobj}.{}",
                postgres_protocol::escape::escape_identifier(ident)
            );
        }
        Rule::bool | Rule::integer | Rule::number | Rule::compares | Rule::jsonpath_unsigned => {
            res += pair.as_str()
        }
        Rule::equals => {
            res += match pair.as_str() {
                "==" => "=",
                "!=" => "<>",
                v => v,
            };
        }
        Rule::logic_and => {
            res += " AND ";
        }
        Rule::logic_or => {
            res += " OR ";
        }
        Rule::logic_not => {
            res += "NOT ";
        }
        Rule::get_hash_fn => {
            res += &format!(
                "{curobj}.\"hashes\"->>{}",
                escape_as_constant_string(pair.into_inner().next().unwrap().0)?
            );
        }
        Rule::has_symbol_fn => {
            let argument = pair.into_inner().next().unwrap();
            match argument.as_rule() {
                Rule::string => {
                    res += &format!(
                        "{curobj}.\"result\"->'ok'->'symbols'?{}",
                        escape_as_constant_string(argument.0)?
                    );
                }
                Rule::func_arg_regex | Rule::func_arg_iregex | Rule::func_arg_starts_with => {
                    res += &format!(
                        "{curobj}.\"result\"->'ok'->'symbols'@?'$?(@{})'",
                        to_sql(argument, rec, single_workid)?
                    );
                }
                _ => unreachable!(),
            }
        }
        Rule::has_error_fn => {
            if let Some(argument) = pair.into_inner().next() {
                match argument.as_rule() {
                    Rule::string => {
                        res += &format!(
                            "{curobj}.\"result\"->>'error'={}",
                            escape_as_constant_string(argument.0)?
                        );
                    }
                    Rule::func_arg_regex | Rule::func_arg_iregex | Rule::func_arg_starts_with => {
                        res += &format!(
                            "{curobj}.\"result\"->'error'@?'$?(@{})'",
                            to_sql(argument, rec, single_workid)?
                        );
                    }
                    _ => unreachable!(),
                }
            } else {
                res += &format!("{curobj}.\"result\"?'error'");
            }
        }
        Rule::has_name_fn => {
            let argument = pair.into_inner().next().unwrap();
            let condition = match argument.as_rule() {
                Rule::string => {
                    format!("=={}", escape_as_json_string(argument.0)?)
                }
                Rule::func_arg_regex | Rule::func_arg_iregex | Rule::func_arg_starts_with => {
                    to_sql(argument, rec, single_workid)?
                }
                _ => unreachable!(),
            };
            res += &format!(
                    "exists(SELECT 1 FROM rels WHERE child = {curobj}.\"id\" AND (props @? '$ ? (@.name{condition} || @.names[*]{condition})'))",
                );
        }
        Rule::has_descendant_fn
        | Rule::has_ancestor_fn
        | Rule::count_ancestors_fn
        | Rule::count_descendants_fn => {
            let (fnname, count) = match pair.as_rule() {
                Rule::has_descendant_fn => ("descendants_of", false),
                Rule::has_ancestor_fn => ("ancestors_of", false),
                Rule::count_descendants_fn => ("descendants_of", true),
                Rule::count_ancestors_fn => ("ancestors_of", true),
                _ => unreachable!(),
            };
            let mut inner = pair.into_inner();
            let node_def = match inner.next() {
                Some(p) => format!(" AND {}", to_sql(p, rec + 1, single_workid)?),
                None => String::new(),
            };
            let maxdepth_def = inner
                .next()
                .map(|depth| format!(", 1, {}", depth.as_str()))
                .unwrap_or(", 1, 1000".to_string());
            let header = if count {
                "(SELECT count(*)"
            } else {
                "exists(SELECT 1"
            };
            res += &format!(
                "{header} FROM objects AS {nextobj} WHERE {match_work} AND id IN (SELECT {fnname}({curobj}.\"id\"{maxdepth_def})){node_def})"
            );
        }
        Rule::has_child_fn | Rule::count_children_fn => {
            let count = matches!(pair.as_rule(), Rule::count_children_fn);
            let header = if count {
                "(SELECT count(*)"
            } else {
                "exists(SELECT 1"
            };
            let mut inner = pair.into_inner();
            let node_def = match inner.next() {
                Some(p) => format!(" AND {}", to_sql(p, rec + 1, single_workid)?),
                None => String::new(),
            };
            res += &format!(
                "{header} FROM objects AS {nextobj} WHERE {match_work} AND id IN (SELECT child FROM rels WHERE parent = {curobj}.\"id\"){node_def})"
            );
        }
        Rule::has_root_fn => {
            res += &format!(
                "exists(SELECT 1 FROM objects AS {nextobj} WHERE {match_work} AND is_entry AND {})",
                to_sql(pair.into_inner().next().unwrap(), rec + 1, single_workid)?
            );
        }
        Rule::has_parent_fn => {
            res += &format!(
                "exists(SELECT 1 FROM objects AS {nextobj} WHERE {match_work} AND id = (SELECT parent FROM rels WHERE child = {curobj}.\"id\") AND {})",
                to_sql(pair.into_inner().next().unwrap(), rec + 1, single_workid)?
            );
        }
        Rule::has_sibling_fn | Rule::count_siblings_fn => {
            let count = matches!(pair.as_rule(), Rule::count_siblings_fn);
            let header = if count {
                "(SELECT count(*)"
            } else {
                "exists(SELECT 1"
            };
            let mut inner = pair.into_inner();
            let node_def = match inner.next() {
                Some(p) => format!(" AND {}", to_sql(p, rec + 1, single_workid)?),
                None => String::new(),
            };
            res += &format!(
                "{header} FROM objects AS {nextobj} WHERE {match_work} AND id IN (SELECT child FROM rels WHERE parent = (select parent from rels where child = {curobj}.\"id\") AND child <> {curobj}.\"id\"){node_def})"
            );
        }
        Rule::is_leaf_fn => {
            res += &format!("( NOT exists(SELECT 1 FROM rels WHERE parent = {curobj}.\"id\") )");
        }
        Rule::date_string => {}
        Rule::date => {
            let input = pair.as_str();
            if time::Date::parse(input, &time::format_description::well_known::Iso8601::DATE)
                .is_err()
            {
                let error = pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Valid date".to_string(),
                    },
                    pair.as_span(),
                );
                return Err(Box::new(error));
            }
            res += input;
        }
        Rule::time => {
            let input = pair.as_str();
            if time::Time::parse(input, &time::format_description::well_known::Iso8601::TIME)
                .is_err()
            {
                let error = pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Valid time".to_string(),
                    },
                    pair.as_span(),
                );
                return Err(Box::new(error));
            }
            res += input;
        }
        Rule::datetime => {
            let mut inner = pair.into_inner();
            res += &to_sql(inner.next().unwrap(), rec + 1, single_workid)?;
            res += " ";
            res += &to_sql(inner.next().unwrap(), rec + 1, single_workid)?;
        }
        Rule::date_range_fn => {
            let mut inner = pair.into_inner();
            let start = to_sql(inner.next().unwrap(), rec + 1, single_workid)?;
            let end_pair = inner.next().unwrap();
            let interval = match end_pair.as_rule() {
                Rule::date => "1",
                Rule::datetime => "INTERVAL '1 seconds'",
                _ => unreachable!(),
            };
            let end = to_sql(end_pair, rec + 1, single_workid)?;
            res += &format!(
                "{curobj}.t BETWEEN '{start}' AND (DATE '{end}'+{interval}-INTERVAL '1 microseconds')"
            );
        }
        Rule::date_since_fn => {
            let mut inner = pair.into_inner();
            let start = to_sql(inner.next().unwrap(), rec + 1, single_workid)?;
            res += &format!("{curobj}.t >= '{start}'",);
        }
        Rule::match_object_meta_fn
        | Rule::has_object_meta_fn
        | Rule::match_relation_meta_fn
        | Rule::has_relation_meta_fn => {
            let (object_meta, check_condition) = match pair.as_rule() {
                Rule::match_object_meta_fn => (true, true),
                Rule::match_relation_meta_fn => (false, true),
                Rule::has_object_meta_fn => (true, false),
                Rule::has_relation_meta_fn => (false, false),
                _ => unreachable!(),
            };
            let mut inner = pair.into_inner();
            let path = &to_sql(inner.next().unwrap(), rec, single_workid)?;

            let mut negate_query = false;
            let (jsonpath, match_length): (String, Option<String>) = if check_condition {
                let pair = inner.next().unwrap();
                if pair.as_rule() == Rule::jsonpath_match_length {
                    let jsonpath = format!(
                        r#"$.ok.object_metadata{} ? (@.type() == "string")"#,
                        &path[1..]
                    );
                    let inner = pair.into_inner();
                    let mut match_length = String::new();
                    for pair in inner {
                        match_length += " ";
                        match_length += &to_sql(pair, rec + 1, single_workid)?
                    }
                    (jsonpath, Some(match_length))
                } else {
                    let mut operator = to_sql(pair, rec, single_workid)?;
                    if ["!=", "<>"].contains(&operator.as_str()) {
                        operator = "==".to_string();
                        negate_query = true;
                    }
                    let mut compare_two_variables = false;
                    let value = match inner.next() {
                        Some(pair) => {
                            let r = pair.as_rule();
                            compare_two_variables = r == Rule::jsonpath_path_simple;
                            if r == Rule::string {
                                escape_as_json_string(pair.0)?
                            } else {
                                to_sql(pair, rec, single_workid)?
                            }
                        }
                        None => String::new(),
                    };
                    let jsonpath = if compare_two_variables {
                        let path1 = format!("@{}", &path[1..]);
                        let path2 = format!("@{}", &value[1..]);
                        format!("$ ? ({path1}{operator}{path2})")
                    } else {
                        format!("{path} ? (@!=null && @{operator}{value})")
                    };
                    (jsonpath, None)
                }
            } else {
                (format!("{path} ? (@!=null)"), None)
            };
            if let Some(match_length) = match_length {
                res += &format!("(exists (SELECT 1 FROM jsonb_path_query({curobj}.result, '{jsonpath}') AS value WHERE length(value #>> '{{}}'){match_length}))");
            } else {
                if negate_query {
                    res += "NOT "
                }
                res += "(";
                if object_meta {
                    res += &format!(
                        "{curobj}.result @? '$.ok.object_metadata{}' AND ",
                        &path[1..]
                    );
                    res += &format!("{curobj}.result->'ok'->'object_metadata' @? '{jsonpath}'");
                } else {
                    res += &format!("(exists (SELECT 1 FROM rels WHERE child = {curobj}.id AND  props @? '{jsonpath}'))");
                }
                res += ")";
            }
        }
        Rule::func_arg_regex | Rule::func_arg_iregex => {
            let flag = match pair.as_rule() {
                Rule::func_arg_regex => "",
                Rule::func_arg_iregex => " flag \"i\"",
                _ => unreachable!(),
            };
            let mut inner = pair.into_inner();
            let regex = escape_as_json_string(inner.next().unwrap().0)?;
            res += &format!(" like_regex {regex}{flag}");
        }
        Rule::func_arg_starts_with => {
            let mut inner = pair.into_inner();
            res += &format!(
                " starts with {}",
                escape_as_json_string(inner.next().unwrap().0)?
            );
        }
        Rule::jsonpath_path_simple => {
            res += "$";
            for p in pair.into_inner() {
                res += &to_sql(p, rec, single_workid)?;
            }
        }
        Rule::jsonpath_ident => {
            let raw = pair.as_str().trim().to_string();
            if let Some(pair) = pair.0.into_inner().next() {
                if pair.as_rule() != Rule::string_regular {
                    unreachable!();
                }
                res += &escape_as_json_string(pair)?;
            } else {
                res += &raw;
            }
        }
        Rule::jsonpath_equals => {
            res += pair.as_str();
        }
        Rule::jsonpath_selector_identifier => {
            res += ".";
            let mut inner = pair.into_inner();
            res += &to_sql(inner.next().unwrap(), rec, single_workid)?;
        }
        Rule::jsonpath_selector_index => {
            let mut inner = pair.into_inner();
            res += &format!(
                "[{}]",
                to_sql(inner.next().unwrap(), rec, single_workid)?.trim()
            );
        }
        Rule::match_pattern_fn => {
            let mut inner = pair.into_inner().next().unwrap().into_inner();
            let mut pattern_pair = inner.next().unwrap();
            let prefix = if pattern_pair.as_rule() == Rule::clam_offset {
                let prefix = format!("0:{}:", pattern_pair.as_str());
                pattern_pair = inner.next().unwrap();
                prefix
            }
            else {
                "0:*:".to_string()
            };
            let hex_signature = match pattern_pair.as_rule() {
                Rule::clam_hex_signature => {
                    validate_hex_signature(&pattern_pair.0)?;
                    pattern_pair.as_str().to_string()
                }
                Rule::string => {
                    let str = rules::unescape_string(pattern_pair.0)?;
                    hex::encode(str)
                }
                _ => unreachable!("Unexpected rule {:?}", pattern_pair.as_rule())
            };
            let hash = hash_sha1(&[&prefix, &hex_signature], 16);
            let signature_name = format!("ContexQL.Pattern.{hash}");
            res += &format!(
                "{curobj}.\"result\"->'ok'->'symbols'?{}",
                postgres_protocol::escape::escape_literal(&signature_name)
            );
        }
        Rule::jsonpath_match_simple
        | Rule::jsonpath_match_length
        | Rule::jsonpath_match_simple_equals
        | Rule::jsonpath_match_simple_compares
        | Rule::char
        | Rule::node_primary
        | Rule::EOI
        | Rule::COMMENT
        | Rule::WHITESPACE => {}
        Rule::clam_pattern
        | Rule::clam_hex_signature
        | Rule::clam_hex_signature_alt
        | Rule::clam_hex_signature_byte
        | Rule::clam_offset
        | Rule::clam_hex_alternative
        | Rule::clam_hex_alternative_singlebyte
        | Rule::clam_hex_alternative_multibyte
        | Rule::clam_hex_alternative_multibyte_part
        | Rule::clam_hex_alternative_generic
        | Rule::clam_hex_subsignature
        | Rule::clam_hex_splitter
        | Rule::clam_hex_signature_byte_simple
        | Rule::clam_hex_wildcard_repetition
        // | Rule::clam_ndb_signature
        // | Rule::clam_ldb_signature
        // | Rule::clam_target_description_block_part
        // | Rule::clam_target_description_block
        // | Rule::clam_logical_expression_group
        // | Rule::clam_logical_expression_line
        // | Rule::clam_logical_expression
        // | Rule::clam_subsig
        // | Rule::clam_pcre_set
        // | Rule::clam_pcre
        => unreachable!(),

    }
    Ok(res)
}

fn modify_pest_error(mut error: pest::error::Error<Rule>) -> pest::error::Error<Rule> {
    use pest::error::ErrorVariant;
    let mut change_to_custom_error = false;
    match &mut error.variant {
        ErrorVariant::ParsingError {
            positives,
            negatives,
        } => {
            positives.retain(|v| *v != Rule::COMMENT);
            if positives.is_empty() && negatives.is_empty() {
                change_to_custom_error = true;
            }
        }
        ErrorVariant::CustomError { .. } => {}
    };
    if change_to_custom_error {
        error.variant = ErrorVariant::CustomError {
            message: "comment, function or complete rule".to_string(),
        }
    }
    error
}

pub fn parse_to_sql<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let mut parsed = RuleParser::parse(Rule::rule, expr.as_ref()).map_err(modify_pest_error)?;
    let parsed = parsed.next().unwrap(); // cannot fail: rule matches from SOI to EOI
    let res = to_sql(PairWrapper(parsed), 0, false);
    debug!("parse_to_sql({}) => {:?}", expr, res);
    res
}

pub fn parse_to_sql_single_work<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let mut parsed = RuleParser::parse(Rule::rule, expr.as_ref()).map_err(modify_pest_error)?;
    let parsed = parsed.next().unwrap(); // cannot fail: rule matches from SOI to EOI
    let res = to_sql(PairWrapper(parsed), 0, true);
    debug!("parse_to_sql_single_work({}) => {:?}", expr, res);
    res
}

pub fn parse_and_extract_clam_signatures<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<Vec<String>, Box<pest::error::Error<Rule>>> {
    let mut result = Vec::new();
    let mut parsed = RuleParser::parse(Rule::rule, expr.as_ref()).map_err(modify_pest_error)?;
    let parsed = parsed.next().unwrap(); // cannot fail: rule matches from SOI to EOI
    extract_clam_signatures(parsed, &mut result);
    Ok(result)
}

fn extract_clam_signatures(pair: pest::iterators::Pair<Rule>, result: &mut Vec<String>) {
    if pair.as_rule() == Rule::clam_pattern {
        let mut inner = pair.into_inner();
        let mut pattern_pair = inner.next().unwrap();
        let prefix = if pattern_pair.as_rule() == Rule::clam_offset {
            let prefix = format!("0:{}:", pattern_pair.as_str());
            pattern_pair = inner.next().unwrap();
            prefix
        } else {
            "0:*:".to_string()
        };
        let hex_signature = match pattern_pair.as_rule() {
            Rule::clam_hex_signature => pattern_pair.as_str().to_string(),
            Rule::string => {
                let str = rules::unescape_string(pattern_pair).unwrap_or_default();
                hex::encode(str)
            }
            _ => unreachable!("Unexpected rule {:?}", pattern_pair.as_rule()),
        };
        let hash = hash_sha1(&[&prefix, &hex_signature], 16);
        result.push(format!("ContexQL.Pattern.{hash}:{prefix}{hex_signature}"));
        return;
    }
    let inner = pair.into_inner();
    for pair in inner {
        extract_clam_signatures(pair, result);
    }
}

fn escape_as_constant_string(
    pair: pest::iterators::Pair<Rule>,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let input = rules::unescape_string(pair)?.replace('\0', "\u{f2b3}");
    Ok(postgres_protocol::escape::escape_literal(&input))
}

fn escape_as_json_string(
    pair: pest::iterators::Pair<Rule>,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let input = rules::unescape_string(pair)?.replace('\0', "\u{f2b3}");
    let mut result = '"'.to_string();
    for c in input.chars() {
        match c {
            '\'' => result += "''",
            '"' => result += "\\\"",
            '\n' => result += "\\n",
            '\r' => result += "\\r",
            '\t' => result += "\\t",
            '\\' => result += "\\\\",
            '/' => result += "\\/",
            '\u{0008}' => result += "\\b",
            '\u{000C}' => result += "\\f",
            _ => {
                if c.is_control() {
                    let code = c as u32;
                    if let Ok(code) = u16::try_from(code) {
                        result += &format!("\\u{code:04X}");
                    } else {
                        // assume all control characters are contained in UTF16
                        unreachable!("Unexpected control character")
                    }
                } else {
                    result.push(c);
                }
            }
        }
    }
    result.push('"');
    Ok(result)
}

fn validate_hex_signature(
    hex_signature: &Pair<'_, Rule>,
) -> Result<(), Box<pest::error::Error<Rule>>> {
    let mut inner = hex_signature.clone().into_inner();
    let mut allow_singlehex = false;
    loop {
        // safe, rule force subsig existence
        let subsig = inner.next().unwrap();
        let subsig_span = subsig.as_span();
        let mut starts_with_hex = None;
        let mut ends_with_hex = false;
        let mut multi_hex = false;
        for pair in subsig.into_inner() {
            match pair.as_rule() {
                Rule::clam_hex_alternative => {
                    if starts_with_hex.is_none() {
                        starts_with_hex = Some(false);
                    }
                    // safe, clam_hex_alternative must contain children
                    let alternative = pair.into_inner().next().unwrap();
                    match alternative.as_rule() {
                        Rule::clam_hex_alternative_multibyte
                            if alternative.as_str().starts_with("!") =>
                        {
                            let span = alternative.as_span();
                            let mut inner = alternative.into_inner();
                            // safe, clam_hex_alternative_multibyte cannot be empty
                            let len = inner.next().unwrap().as_str().len();
                            for part in inner {
                                if part.as_str().len() != len {
                                    return Err(Box::new(pest::error::Error::new_from_span(
                                        pest::error::ErrorVariant::CustomError {
                                            message: "All members to be the same length"
                                                .to_string(),
                                        },
                                        span,
                                    )));
                                }
                            }
                        }
                        Rule::clam_hex_alternative_multibyte
                        | Rule::clam_hex_alternative_singlebyte
                        | Rule::clam_hex_alternative_generic => {}
                        _ => unreachable!(),
                    }
                    ends_with_hex = false;
                    continue;
                }
                Rule::clam_hex_wildcard_repetition => {
                    if starts_with_hex.is_none() {
                        starts_with_hex = Some(false);
                    }
                    ends_with_hex = false;
                    continue;
                }
                Rule::clam_hex_signature_byte => {}
                _ => unreachable!("Unable to parse {:?}", hex_signature.as_str()),
            }
            if starts_with_hex.is_none() {
                starts_with_hex = Some(!pair.as_str().contains('?'));
            }
            if pair.as_str().contains('?') {
                ends_with_hex = false;
                continue;
            }
            if ends_with_hex {
                multi_hex = true;
            }
            ends_with_hex = true;
        }
        // safe, variable is set during processing of first child
        let starts_with_hex = starts_with_hex.unwrap();
        let splitter = inner.next();
        let splitter_is_square_bracket = {
            if let Some(splitter) = &splitter {
                splitter.as_str().starts_with("[")
            } else {
                false
            }
        };
        // sub-signature must include a block of two static characters
        // [x-y] notation allow to use single static character on one side
        let invalid_subsig = if !multi_hex {
            if (starts_with_hex && allow_singlehex) || (ends_with_hex && splitter_is_square_bracket)
            {
                allow_singlehex = false;
                false
            } else {
                true
            }
        } else {
            allow_singlehex = splitter_is_square_bracket;
            false
        };
        if invalid_subsig {
            return Err(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Sub-signature containing a block of two static bytes".to_string(),
                },
                subsig_span,
            )));
        }
        let Some(splitter) = splitter else {
            break;
        };
        if splitter.as_str() == "*" {
            continue;
        }
        let len = splitter.as_str().len() - 1;
        let splitter_str = &splitter.as_str()[1..len];
        // safe, rule require hyphen
        let (min, max) = splitter_str.split_once('-').unwrap();
        let min = min.parse::<u32>().ok();
        let max = max.parse::<u32>().ok();
        let valid_range = || -> bool {
            if min.is_none() && max.is_none() {
                return false;
            }
            let Some(min) = min else {
                return true;
            };
            let Some(max) = max else {
                return true;
            };
            max >= min
        }();
        if !valid_range {
            return Err(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Valid range".to_string(),
                },
                splitter.as_span(),
            )));
        }
    }
    Ok(())
}

fn hash_sha1(input: &[&str], byte_limit: usize) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    for input in input {
        hasher.update(input.as_bytes());
    }
    let hash = hasher.finalize();
    let slice = &hash.as_slice()[0..byte_limit.min(hash.len())];
    hex::encode(slice)
}

#[cfg(test)]
use std::error::Error;
use std::fmt::Debug;

#[cfg(test)]
fn parse_rule(r: rules::Rule, input: &str) -> Result<String, Box<dyn Error>> {
    let mut parsed = RuleParser::parse(r, input)?;
    let parsed = parsed.next().ok_or("Pairs<Rule> is empty")?;
    Ok(to_sql(PairWrapper(parsed), 0, false)?)
}

#[test]
fn test_jsonpath() {
    use rules::Rule::functions;

    assert_eq!(
        parse_rule(functions, "@match_object_meta(/*comment*/$x == 1)").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @==1)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x.y.z /*comment*/== 1)").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x.y.z' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x.y.z ? (@!=null && @==1)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x[0].z // single line comment
        == 1)").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x[0].z' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x[0].z ? (@!=null && @==1)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta(/*
        multiline comment
        */$x[0][0] == 1)").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x[0][0]' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x[0][0] ? (@!=null && @==1)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x > 1)").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @>1)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x == \"x\")").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @==\"x\")')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x != true)").unwrap(),
        "NOT (\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @==true)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x regex(\"^x\"))").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @ like_regex \"^x\")')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x iregex(\"^x\"))").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @ like_regex \"^x\" flag \"i\")')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x starts_with(\"x\"))").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @ starts with \"x\")')"
    );
    assert_eq!(
        parse_rule(
            Rule::rule,
            "@has_object_meta($possible_passwords) && !@has_object_meta($programming_language)"
        )
        .unwrap(),
        "FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.possible_passwords' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.possible_passwords ? (@!=null)') AND NOT (\"objects_0\".result @? '$.ok.object_metadata.programming_language' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.programming_language ? (@!=null)'))"
    );
    assert_eq!(
        parse_rule(Rule::functions, "@match_object_meta($a.b.c.len()==1)").unwrap(),
        "(exists (SELECT 1 FROM jsonb_path_query(\"objects_0\".result, '$.ok.object_metadata.a.b.c ? (@.type() == \"string\")') AS value WHERE length(value #>> '{}') = 1))"
    );
    assert_eq!(
        parse_rule(Rule::functions, "@match_object_meta($a.b[0].c.len()!=1)").unwrap(),
        "(exists (SELECT 1 FROM jsonb_path_query(\"objects_0\".result, '$.ok.object_metadata.a.b[0].c ? (@.type() == \"string\")') AS value WHERE length(value #>> '{}') <> 1))"
    );
}

#[test]
fn test_escaping() {
    use rules::Rule::constant_string;
    assert_eq!(
        parse_rule(constant_string, r#""string""#).unwrap(),
        "'string'"
    );
    assert_eq!(
        parse_rule(constant_string, r#""str'ing""#).unwrap(),
        "'str''ing'"
    );
    assert_eq!(
        parse_rule(constant_string, r#""str\"ing""#).unwrap(),
        "'str\"ing'"
    );
    assert_eq!(
        parse_rule(constant_string, r#""str\ting""#).unwrap(),
        "'str\ting'"
    );
    assert_eq!(
        parse_rule(constant_string, r#""str\ning""#).unwrap(),
        "'str\ning'"
    );
    assert_eq!(parse_rule(constant_string, r#""\u0041""#).unwrap(), "'A'");
    assert_eq!(
        parse_rule(constant_string, r#""1' UNION SELECT 'a'; -- -'""#).unwrap(),
        "'1'' UNION SELECT ''a''; -- -'''"
    );
    assert_eq!(
        parse_rule(constant_string, r#""a\"\\/\n\r\t\u0041z""#).unwrap(),
        " E'a\"\\\\/\n\r\tAz'"
    );
    assert_eq!(
        parse_to_sql(r#"@match_object_meta($x == "a'b_\n_z")"#),
        Ok("FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @==\"a''b_\\n_z\")'))".to_string()),
    );
    assert_eq!(
        parse_to_sql(r#"@has_object_meta($"injection'; delete from db; --")"#),
        Ok("FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.\"injection''; delete from db; --\"' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.\"injection''; delete from db; --\" ? (@!=null)'))".to_string()),
    );
}

#[test]
fn test_date_functions() {
    use rules::Rule::rule;
    assert_eq!(
        parse_rule(rule, r#"@date_since("2000-01-01")"#).unwrap(),
        "FROM objects AS \"objects_0\" WHERE (\"objects_0\".t >= '2000-01-01')"
    );
    assert!(parse_rule(rule, r#"@date_since("2000-02-30")"#).is_err());
    assert_eq!(
        parse_rule(rule, r#"@date_since("2000-01-01 11:22:33")"#).unwrap(),
        "FROM objects AS \"objects_0\" WHERE (\"objects_0\".t >= '2000-01-01 11:22:33')"
    );
    assert!(parse_rule(rule, r#"@date_since("2000-02-30 11:60:33")"#).is_err());
    assert_eq!(
        parse_rule(rule, r#"@date_range("2000-01-01", "2000-01-01")"#).unwrap(),
        "FROM objects AS \"objects_0\" WHERE (\"objects_0\".t BETWEEN '2000-01-01' AND (DATE '2000-01-01'+1-INTERVAL '1 microseconds'))"
    );
    assert_eq!(
        parse_rule(rule, r#"@date_range("2000-01-01 00:00:00", "2000-01-01 00:00:00")"#).unwrap(),
        "FROM objects AS \"objects_0\" WHERE (\"objects_0\".t BETWEEN '2000-01-01 00:00:00' AND (DATE '2000-01-01 00:00:00'+INTERVAL '1 seconds'-INTERVAL '1 microseconds'))"
    );
}

#[test]
fn test_clam_signatures() {
    use rules::Rule::rule;
    let input = "@match_pattern(deadbeef)";
    assert_eq!(
        parse_rule(rule, input).unwrap(),
        "FROM objects AS \"objects_0\" WHERE (\"objects_0\".\"result\"->'ok'->'symbols'?'ContexQL.Pattern.17b1d1bc76fbe993810df1a1c50a35a5')"
    );
    let signatures: Vec<String> = parse_and_extract_clam_signatures(input).unwrap();
    assert_eq!(
        signatures,
        ["ContexQL.Pattern.17b1d1bc76fbe993810df1a1c50a35a5:0:*:deadbeef"]
    );

    let input = r"@match_pattern(deadbeef)
                        && @has_child(@match_pattern(EOF-50:e80?000000{-10}5bb9??(00|01)0000{-10}03d9{-10}8b1b{-25}3bd977{-10}cd20))";
    assert_eq!(
        parse_rule(rule, input).unwrap(),
        "FROM objects AS \"objects_0\" WHERE (\"objects_0\".\"result\"->'ok'->'symbols'?'ContexQL.Pattern.17b1d1bc76fbe993810df1a1c50a35a5' AND exists(SELECT 1 FROM objects AS \"objects_1\" WHERE \"objects_1\".work_id = \"objects_0\".work_id AND id IN (SELECT child FROM rels WHERE parent = \"objects_0\".\"id\") AND (\"objects_1\".\"result\"->'ok'->'symbols'?'ContexQL.Pattern.5060229c6fb892c23264eedd9eebf9f3')))"
    );
    let signatures: Vec<String> = parse_and_extract_clam_signatures(input).unwrap();
    assert_eq!(
        signatures,
        [
            r"ContexQL.Pattern.17b1d1bc76fbe993810df1a1c50a35a5:0:*:deadbeef",
            r"ContexQL.Pattern.5060229c6fb892c23264eedd9eebf9f3:0:EOF-50:e80?000000{-10}5bb9??(00|01)0000{-10}03d9{-10}8b1b{-25}3bd977{-10}cd20"
        ]
    );

    let input = r"@match_pattern(acab)";
    assert_eq!(
        parse_rule(rule, input).unwrap(),
        r#"FROM objects AS "objects_0" WHERE ("objects_0"."result"->'ok'->'symbols'?'ContexQL.Pattern.9a349c208c5e13c6bafdee32d17cf71e')"#
    );
    let signatures: Vec<String> = parse_and_extract_clam_signatures(input).unwrap();
    assert_eq!(
        signatures,
        ["ContexQL.Pattern.9a349c208c5e13c6bafdee32d17cf71e:0:*:acab"]
    );

    let input = r#"@match_pattern(*:696e766f696365)"#;
    assert_eq!(
        parse_rule(rule, input).unwrap(),
        r#"FROM objects AS "objects_0" WHERE ("objects_0"."result"->'ok'->'symbols'?'ContexQL.Pattern.545e8c5d3f61e38fe8d64e05727d36c1')"#
    );
    let signatures: Vec<String> = parse_and_extract_clam_signatures(input).unwrap();
    assert_eq!(
        signatures,
        ["ContexQL.Pattern.545e8c5d3f61e38fe8d64e05727d36c1:0:*:696e766f696365"]
    );

    let input = r#"@match_pattern(*:r"invoice")"#;
    assert_eq!(
        parse_rule(rule, input).unwrap(),
        r#"FROM objects AS "objects_0" WHERE ("objects_0"."result"->'ok'->'symbols'?'ContexQL.Pattern.545e8c5d3f61e38fe8d64e05727d36c1')"#
    );
    let signatures: Vec<String> = parse_and_extract_clam_signatures(input).unwrap();
    assert_eq!(
        signatures,
        ["ContexQL.Pattern.545e8c5d3f61e38fe8d64e05727d36c1:0:*:696e766f696365"]
    );
}

#[test]
fn test_comments() {
    let queries = [
        "/**/is_entry/**/",
        "/**/is_entry/**/==/**/false/**/",
        "is_entry/**/&&/**/size/**/==/**/1/**/",
        "/**/(/**/is_entry/**/&&/**/(/**/is_entry/**/)/**/)/**/",
        r#"/**/@has_name/**/(/**/"name"/**/)/**/"#,
        r#"/**/@has_name/**/(/**/regex(/**/"name"/**/)/**/)/**/"#,
        r#"/**/@has_name/**/(/**/iregex(/**/"name"/**/)/**/)/**/"#,
        r#"/**/@has_name/**/(/**/starts_with(/**/"name"/**/)/**/)/**/"#,
        r#"@date_since(/**/"2024-09-26"/**/)"#,
        r#"@date_range(/**/"2024-09-26"/**/,/**/"2024-09-26"/**/)"#,
        "@match_object_meta(/**/$/**/key/**/./**/key/**/[/**/0/**/]/**/==/**/1/**/)",
        r#"@match_object_meta($key/**/regex(/**/"value"/**/)/**/)"#,
        r#"@match_pattern(/**/"asdf"/**/)"#,
        r#"@match_pattern(/**/aabb*ccdd[2-4]ee/**/)"#,
    ];
    for query in queries {
        match parse_to_sql(query) {
            Ok(sql) => {
                if sql.contains("/*") {
                    println!("QUERY: {query}");
                    println!("SQL: {sql}");
                    panic!("There were errors");
                }
            }
            Err(err) => {
                println!("QUERY: {query}");
                println!("ERR: {err}");
                panic!("There were errors");
            }
        }
    }
}
