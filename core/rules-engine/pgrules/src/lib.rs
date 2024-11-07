use pest::Parser;
use rules::{Rule, RuleParser};
use tracing::{debug, trace};

pub struct PgRule(RuleParser);

fn to_sql(
    pair: pest::iterators::Pair<Rule>,
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
            let string = escape_as_constant_string(pair.into_inner().next().unwrap())?;
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
        Rule::bool | Rule::integer | Rule::number | Rule::compares => res += pair.as_str(),
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
                escape_as_constant_string(pair.into_inner().next().unwrap())?
            );
        }
        Rule::has_symbol_fn => {
            let argument = pair.into_inner().next().unwrap();
            match argument.as_rule() {
                Rule::string => {
                    res += &format!(
                        "{curobj}.\"result\"->'ok'->'symbols'?{}",
                        escape_as_constant_string(argument)?
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
                            escape_as_constant_string(argument)?
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
                    format!("=={}", escape_as_json_string(argument)?)
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
                        message: "Invalid date".to_string(),
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
                        message: "Invalid time".to_string(),
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
            let jsonpath = if check_condition {
                let mut operator = to_sql(inner.next().unwrap(), rec, single_workid)?;
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
                            escape_as_json_string(pair)?
                        } else {
                            to_sql(pair, rec, single_workid)?
                        }
                    }
                    None => String::new(),
                };
                if compare_two_variables {
                    let path1 = format!("@{}", &path[1..]);
                    let path2 = format!("@{}", &value[1..]);
                    format!("$ ? ({path1}{operator}{path2})")
                } else {
                    format!("{path} ? (@!=null && @{operator}{value})")
                }
            } else {
                format!("{path} ? (@!=null)")
            };
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
        Rule::func_arg_regex | Rule::func_arg_iregex => {
            let flag = match pair.as_rule() {
                Rule::func_arg_regex => "",
                Rule::func_arg_iregex => " flag \"i\"",
                _ => unreachable!(),
            };
            let mut inner = pair.into_inner();
            let regex = escape_as_json_string(inner.next().unwrap())?;
            res += &format!(" like_regex {regex}{flag}");
        }
        Rule::func_arg_starts_with => {
            let mut inner = pair.into_inner();
            res += &format!(
                " starts with {}",
                escape_as_json_string(inner.next().unwrap())?
            );
        }
        Rule::jsonpath_path_simple => {
            res += "$";
            for p in pair.into_inner() {
                res += &to_sql(p, rec, single_workid)?;
            }
        }
        Rule::jsonpath_ident => {
            let raw = pair.as_str().trim();
            if let Some(pair) = pair.into_inner().next() {
                if pair.as_rule() != Rule::string_regular {
                    unreachable!();
                }
                res += &escape_as_json_string(pair)?;
            } else {
                res += raw;
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
        Rule::jsonpath_match_simple
        | Rule::jsonpath_match_simple_equals
        | Rule::jsonpath_match_simple_compares => {}
        Rule::EOI | Rule::char | Rule::node_primary | Rule::WHITESPACE => {}
    }
    Ok(res)
}

pub fn parse_to_sql<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let mut parsed = RuleParser::parse(Rule::rule, expr.as_ref())?;
    let parsed = parsed.next().unwrap(); // cannot fail: rule matches from SOI to EOI
    let res = to_sql(parsed, 0, false);
    debug!("parse_to_sql({}) => {:?}", expr, res);
    res
}

pub fn parse_to_sql_single_work<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let mut parsed = RuleParser::parse(Rule::rule, expr.as_ref())?;
    let parsed = parsed.next().unwrap(); // cannot fail: rule matches from SOI to EOI
    let res = to_sql(parsed, 0, true);
    debug!("parse_to_sql_single_work({}) => {:?}", expr, res);
    res
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

#[cfg(test)]
use std::error::Error;

#[cfg(test)]
fn parse_rule(r: rules::Rule, input: &str) -> Result<String, Box<dyn Error>> {
    let mut parsed = RuleParser::parse(r, input)?;
    let parsed = parsed.next().ok_or("Pairs<Rule> is empty")?;
    Ok(to_sql(parsed, 0, false)?)
}

#[test]
fn test_jsonpath() {
    use rules::Rule::functions;

    assert_eq!(
        parse_rule(functions, "@match_object_meta($x == 1)").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x ? (@!=null && @==1)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x.y.z == 1)").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x.y.z' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x.y.z ? (@!=null && @==1)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x[0].z == 1)").unwrap(),
        "(\"objects_0\".result @? '$.ok.object_metadata.x[0].z' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.x[0].z ? (@!=null && @==1)')"
    );
    assert_eq!(
        parse_rule(functions, "@match_object_meta($x[0][0] == 1)").unwrap(),
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
