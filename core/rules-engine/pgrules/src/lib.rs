use pest::{iterators::Pair, Parser, Span};
use rules::{unescape_string, Rule, RuleParser};
use std::{collections::HashMap, fmt::Debug};
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

enum VariableValue {
    Bool(String),
    Number(String),
    String(String),
    DateTime {
        date_string: String,
        interval: String,
    },
    ClamPattern {
        name: String,
        //pattern: String,
    },
}

pub fn to_sql(
    pair: PairWrapper,
    rec: u32,
    single_workid: bool,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let mut variables: HashMap<String, VariableValue> = HashMap::new();
    to_sql_inner(pair, rec, single_workid, &mut variables)
}

fn parse_variable_definition(
    pair: PairWrapper,
    variables: &mut HashMap<String, VariableValue>,
) -> Result<(), Box<pest::error::Error<Rule>>> {
    assert!(pair.as_rule() == Rule::variable_definition);
    let mut inner = pair.into_inner();
    //safe
    let name_pair = inner.next().unwrap();
    let name = name_pair.as_str();
    if variables.contains_key(name) {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: format!("Variable {name} is already defined"),
            },
            name_pair.as_span(),
        )
        .into());
    }
    //safe
    let value_pair = inner.next().unwrap().into_inner().next().unwrap();
    let value_rule = value_pair.as_rule();
    //safe
    let value_pair = value_pair.into_inner().next().unwrap();
    let value = match value_rule {
        Rule::variable_bool => {
            let value = to_sql_inner(value_pair, 1, false, variables)?;
            VariableValue::Bool(value)
        }
        Rule::variable_number => {
            let value = to_sql_inner(value_pair, 1, false, variables)?;
            VariableValue::Number(value)
        }
        Rule::variable_string => {
            let value = unescape_string(value_pair.0)?;
            VariableValue::String(value)
        }
        Rule::variable_date => {
            let interval = match value_pair.as_rule() {
                Rule::date => "1",
                Rule::datetime => "INTERVAL '1 seconds'",
                _ => unreachable!(),
            }
            .to_string();
            let date_string = to_sql_inner(value_pair, 1, false, variables)?;
            VariableValue::DateTime {
                date_string,
                interval,
            }
        }
        Rule::variable_clam_pattern => {
            let mut inner = value_pair.into_inner();
            let mut pattern_pair = inner.next().unwrap();
            let prefix = if pattern_pair.as_rule() == Rule::clam_offset {
                let prefix = format!("0:{}:", pattern_pair.as_str());
                pattern_pair = inner.next().unwrap();
                prefix
            } else {
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
                _ => unreachable!("Unexpected rule {:?}", pattern_pair.as_rule()),
            };
            let hash = hash_sha1(&[&prefix, &hex_signature], 16);
            let name = format!("ContexQL.Pattern.{hash}");
            //let pattern = format!("{prefix}{hex_signature}");
            VariableValue::ClamPattern { name, /*pattern*/ }
        }
        _ => unreachable!("rule {:?} => {}", value_pair.as_rule(), value_pair.as_str()),
    };
    variables.insert(name.to_string(), value);
    Ok(())
}

fn to_sql_inner(
    pair: PairWrapper,
    rec: u32,
    single_workid: bool,
    variables: &mut HashMap<String, VariableValue>,
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
                if p.as_rule() == Rule::variable_definition {
                    parse_variable_definition(p, variables)?;
                }
                else {
                    res += &to_sql_inner(p, rec, single_workid, variables)?;
                }
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
            let string = escape_pair_as_constant_string(pair.into_inner().next().unwrap().0)?;
            res += &string;
        }
        Rule::functions
        | Rule::functions_bool
        | Rule::functions_number
        | Rule::functions_string
        | Rule::op
        | Rule::glue => {
            for p in pair.into_inner() {
                res += &to_sql_inner(p, rec, single_workid, variables)?;
            }
        }
        Rule::cond => {
            #[derive(PartialEq)]
            enum Type {
                    Bool,
                    Number,
                    String,
                    Other
                }
            let mut inner = pair.into_inner();
            // safe
            let left = inner.next().unwrap();
            let expected_type = match left.as_rule() {
                Rule::ident_bool | Rule::functions_bool | Rule::bool => Type::Bool,
                Rule::ident_number | Rule::functions_number => Type::Number,
                Rule::ident_string | Rule::functions_string => Type::String,
                _ => unreachable!()
            };
            res += &to_sql_inner(left, rec, single_workid, variables)?;
            if let Some(op) = inner.next() {
                res += &to_sql_inner(op, rec, single_workid, variables)?;
                // safe
                let right = inner.next().unwrap();
                if right.as_rule() == Rule::variable {
                    let variable = get_variable(variables, &right.0)?;
                    let (variable_type, variable_value) = match variable {
                        VariableValue::Bool(v) => (Type::Bool, v.as_str()),
                        VariableValue::Number(v) => (Type::Number, v.as_str()),
                        VariableValue::String(v) => (Type::String, v.as_str()),
                        VariableValue::DateTime { ..  } |
                        VariableValue::ClamPattern { .. } => (Type::Other, ""),
                    };
                    if variable_type != expected_type {
                        return Err(incompatible_variable(right.as_span()));
                    }
                    if variable_type == Type::String {
                        let escaped_string = escape_string_as_constant_string(variable_value);
                        res += &escaped_string;
                    }
                    else {
                        res += variable_value;
                    }
                }
                else {
                    res += &to_sql_inner(right, rec, single_workid, variables)?;
                }
            }
        }
        Rule::node => {
            res += "(";
            for p in pair.into_inner() {
                res += &to_sql_inner(p, rec, single_workid, variables)?;
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
                escape_pair_as_constant_string(pair.into_inner().next().unwrap().0)?
            );
        }
        Rule::has_symbol_fn => {
            let argument = pair.into_inner().next().unwrap();
            match argument.as_rule() {
                Rule::string => {
                    res += &format!(
                        "{curobj}.\"result\"->'ok'->'symbols'?{}",
                        escape_pair_as_constant_string(argument.0)?
                    );
                }
                Rule::variable => {
                    let variable = get_variable(variables, &argument.0)?;
                    let VariableValue::String(variable_value) = variable else {
                        return Err(incompatible_variable(argument.as_span()));
                    };
                    res += &format!(
                        "{curobj}.\"result\"->'ok'->'symbols'?{}",
                        escape_string_as_constant_string(variable_value)
                    );
                }
                Rule::func_arg_regex | Rule::func_arg_iregex | Rule::func_arg_starts_with => {
                    res += &format!(
                        "{curobj}.\"result\"->'ok'->'symbols'@?'$?(@{})'",
                        to_sql_inner(argument, rec, single_workid, variables)?
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
                            escape_pair_as_constant_string(argument.0)?
                        );
                    }
                    Rule::variable => {
                        let variable = get_variable(variables, &argument.0)?;
                        let VariableValue::String(variable_value) = variable else {
                            return Err(incompatible_variable(argument.as_span()));
                        };
                        res += &format!(
                            "{curobj}.\"result\"->>'error'={}",
                            escape_string_as_constant_string(variable_value)
                        );
                    }
                    Rule::func_arg_regex | Rule::func_arg_iregex | Rule::func_arg_starts_with => {
                        res += &format!(
                            "{curobj}.\"result\"->'error'@?'$?(@{})'",
                            to_sql_inner(argument, rec, single_workid, variables)?
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
                    format!("=={}", escape_pair_as_json_string(argument.0)?)
                }
                Rule::variable => {
                    let variable = get_variable(variables, &argument.0)?;
                    let VariableValue::String(variable_value) = variable else {
                        return Err(incompatible_variable(argument.as_span()));
                    };
                    format!("=={}", escape_string_as_json_string(variable_value))
                }
                Rule::func_arg_regex | Rule::func_arg_iregex | Rule::func_arg_starts_with => {
                    to_sql_inner(argument, rec, single_workid, variables)?
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
                Some(p) => format!(" AND {}", to_sql_inner(p, rec + 1, single_workid, variables)?),
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
                Some(p) => format!(" AND {}", to_sql_inner(p, rec + 1, single_workid, variables)?),
                None => String::new(),
            };
            res += &format!(
                "{header} FROM objects AS {nextobj} WHERE {match_work} AND id IN (SELECT child FROM rels WHERE parent = {curobj}.\"id\"){node_def})"
            );
        }
        Rule::has_root_fn => {
            res += &format!(
                "exists(SELECT 1 FROM objects AS {nextobj} WHERE {match_work} AND is_entry AND {})",
                to_sql_inner(pair.into_inner().next().unwrap(), rec + 1, single_workid, variables)?
            );
        }
        Rule::has_parent_fn => {
            res += &format!(
                "exists(SELECT 1 FROM objects AS {nextobj} WHERE {match_work} AND id = (SELECT parent FROM rels WHERE child = {curobj}.\"id\") AND {})",
                to_sql_inner(pair.into_inner().next().unwrap(), rec + 1, single_workid, variables)?
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
                Some(p) => format!(" AND {}", to_sql_inner(p, rec + 1, single_workid, variables)?),
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
            res += &to_sql_inner(inner.next().unwrap(), rec + 1, single_workid, variables)?;
            res += " ";
            res += &to_sql_inner(inner.next().unwrap(), rec + 1, single_workid, variables)?;
        }
        Rule::date_range_fn => {
            let mut inner = pair.into_inner();
            //safe
            let start_pair = inner.next().unwrap();
            let start = if start_pair.as_rule() == Rule::variable {
                let variable = get_variable(variables, &start_pair.0)?;
                let VariableValue::DateTime { date_string, .. } = variable else {
                    return Err(incompatible_variable(start_pair.as_span()));
                };
                date_string.to_string()
            }
            else {
                to_sql_inner(start_pair, rec + 1, single_workid, variables)?
            };
            //safe
            let end_pair = inner.next().unwrap();
            let (end, interval) = if end_pair.as_rule() == Rule::variable {
                let variable = get_variable(variables, &end_pair.0)?;
                let VariableValue::DateTime { date_string, interval } = variable else {
                    return Err(incompatible_variable(end_pair.as_span()));
                };
                (date_string.to_string(), interval.as_str())
            }
            else {
                let interval = match end_pair.as_rule() {
                    Rule::date => "1",
                    Rule::datetime => "INTERVAL '1 seconds'",
                    _ => unreachable!(),
                };
                let end = to_sql_inner(end_pair, rec + 1, single_workid, variables)?;
                (end,interval)
            };
            res += &format!(
                "{curobj}.t BETWEEN '{start}' AND (DATE '{end}'+{interval}-INTERVAL '1 microseconds')"
            );
        }
        Rule::date_since_fn => {
            //safe
            let pair = pair.into_inner().next().unwrap();
            let start = if pair.as_rule() == Rule::variable {
                let variable = get_variable(variables, &pair.0)?;
                let VariableValue::DateTime { date_string, .. } = variable else {
                    return Err(incompatible_variable(pair.as_span()));
                };
                date_string.to_string()
            }
            else {
                to_sql_inner(pair, rec + 1, single_workid, variables)?
            };
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
            let path = &to_sql_inner(inner.next().unwrap(), rec, single_workid, variables)?;

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
                        if pair.as_rule() == Rule::variable {
                            let variable = get_variable(variables, &pair.0)?;
                            let VariableValue::Number(variable_value) = variable else {
                                return Err(incompatible_variable(pair.as_span()));
                            };
                            if variable_value.starts_with('-') {
                                return Err(incompatible_variable(pair.as_span()));
                            }
                            match_length += variable_value;
                        }
                        else {
                            match_length += &to_sql_inner(pair, rec + 1, single_workid, variables)?;
                        }
                    }
                    (jsonpath, Some(match_length))
                } else if pair.as_rule() == Rule::jsonpath_object_match {
                    //safe
                    let pair = pair.into_inner().next().unwrap();
                    let node = to_sql_inner(pair, rec, single_workid, variables)?;
                    (format!("{path} ? (@!=null && {node})"), None)
                } else {
                    let comparison = pair.as_rule() == Rule::compares;
                    let mut operator = to_sql_inner(pair, rec, single_workid, variables)?;
                    if ["!=", "<>"].contains(&operator.as_str()) {
                        operator = "==".to_string();
                        negate_query = true;
                    }
                    let mut compare_two_variables = false;
                    let value = match inner.next() {
                        Some(pair) => {
                            let r = pair.as_rule();
                            compare_two_variables = r == Rule::jsonpath_path_simple;
                            match r {
                                Rule::string => escape_pair_as_json_string(pair.0)?,
                                Rule::variable => {
                                    let variable = get_variable(variables, &pair.0)?;
                                    match variable {
                                        VariableValue::Bool(v) if !comparison => v.to_string(),
                                        VariableValue::Number(v) => v.to_string(),
                                        VariableValue::String(v) if !comparison => escape_string_as_json_string(v),
                                        _ => {
                                            return Err(incompatible_variable(pair.as_span()));
                                        }
                                    }
                                }
                                _ => to_sql_inner(pair, rec, single_workid, variables)?
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
            //safe
            let pair = pair.into_inner().next().unwrap();
            if pair.as_rule() == Rule::variable {
                let variable = get_variable(variables, &pair.0)?;
                let VariableValue::String(variable_value) = variable else {
                    return Err(incompatible_variable(pair.as_span()));
                };
                let regex = escape_string_as_json_string(variable_value);
                res += &format!(" like_regex {regex}{flag}");
            }
            else {
                let regex = escape_pair_as_json_string(pair.0)?;
                res += &format!(" like_regex {regex}{flag}");
            }
        }
        Rule::func_arg_starts_with => {
            //safe
            let pair = pair.into_inner().next().unwrap();
            if pair.as_rule() == Rule::variable {
                let variable = get_variable(variables, &pair.0)?;
                let VariableValue::String(variable_value) = variable else {
                    return Err(incompatible_variable(pair.as_span()));
                };
                let prefix = escape_string_as_json_string(variable_value);
                res += &format!(" starts with {prefix}");
            }
            else {
                let prefix = escape_pair_as_json_string(pair.0)?;
                res += &format!(" starts with {prefix}");
            }
        }
        Rule::jsonpath_path_simple => {
            res += "$";
            for p in pair.into_inner() {
                res += &to_sql_inner(p, rec, single_workid, variables)?;
            }
        }
        Rule::jsonpath_object_match_condition_node => {
            res += "(";
            for p in pair.into_inner() {
                let v = if p.as_rule() == Rule::glue {
                    match p.as_str().to_lowercase().as_str() {
                        "and" => "&&",
                        "or" => "||",
                        v => v,
                    }.to_string()
                } else {
                    to_sql_inner(p, rec, single_workid, variables)?
                };
                res += &v;
            }
            res += ")";
        }
        Rule::jsonpath_object_match_condition_simple => {
            let mut inner = pair.into_inner();
            //safe
            let id = to_sql_inner(inner.next().unwrap(), rec, single_workid, variables)?;
            //safe
            let op_pair = inner.next().unwrap();
            match op_pair.as_rule() {
                Rule::jsonpath_object_match_equals | Rule::jsonpath_object_match_compares=> {
                    let mut inner = op_pair.into_inner();
                    //safe
                    let op_pair = inner.next().unwrap();
                    let op = op_pair.as_str();
                    let (op,negate) = {
                        if ["!=", "<>"].contains(&op) {
                            ("!=", true)
                        }
                        else {
                            (op, false)
                        }
                    };
                    //safe
                    let val_pair = inner.next().unwrap();
                    let mut negate_type = if negate {
                        match val_pair.as_rule() {
                            Rule::string => Some("string"),
                            Rule::number => Some("number"),
                            Rule::bool => Some("boolean"),
                            _ => None
                        }
                    } else {
                        None
                    };
                    let val = match val_pair.as_rule() {
                        Rule::variable => {
                            let variable = get_variable(variables, &val_pair.0)?;
                            if op_pair.as_rule() == Rule::jsonpath_object_match_compares {
                                let VariableValue::Number(val) = variable else {
                                    return Err(incompatible_variable(val_pair.as_span()));
                                };
                                val.to_string()
                            }
                            else {
                                if negate {
                                    negate_type = match variable {
                                        VariableValue::String(_) => Some("string"),
                                        VariableValue::Number(_) => Some("number"),
                                        VariableValue::Bool(_) => Some("boolean"),
                                        _ => None
                                    };
                                }
                                match variable {
                                    VariableValue::Bool(v) => v.to_string(),
                                    VariableValue::Number(v) => v.to_string(),
                                    VariableValue::String(v) => escape_string_as_json_string(v),
                                    VariableValue::DateTime { .. } |
                                    VariableValue::ClamPattern { .. } => return Err(incompatible_variable(val_pair.as_span()))
                                }
                            }
                        }
                        Rule::string => escape_pair_as_json_string(val_pair.0)?,
                        _ => to_sql_inner(val_pair, rec, single_workid, variables)?
                    };
                    if let Some(negate_type) = negate_type {
                        res += &format!(r#"({id}{op}{val} || {id}.type()!="{negate_type}")"#)
                    } else {
                        res += &format!("{id}{op}{val}")
                    }
                }
                Rule::func_arg_regex |
                Rule::func_arg_iregex |
                Rule::func_arg_starts_with => {
                    let func = to_sql_inner(op_pair, rec, single_workid, variables)?;
                    res += &format!("{id}{func}")
                }
                _ => unreachable!()
            };


        }
        Rule::jsonpath_object_match_id => {
            res += "@";
            //safe
            res += &to_sql_inner(pair.into_inner().next().unwrap(), rec,single_workid,variables)?;
        }
        Rule::jsonpath_ident => {
            let raw = pair.as_str().trim().to_string();
            if let Some(pair) = pair.0.into_inner().next() {
                if pair.as_rule() != Rule::string_regular {
                    unreachable!();
                }
                res += &escape_pair_as_json_string(pair)?;
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
            res += &to_sql_inner(inner.next().unwrap(), rec, single_workid, variables)?;
        }
        Rule::jsonpath_selector_index => {
            let mut inner = pair.into_inner();
            res += &format!(
                "[{}]",
                to_sql_inner(inner.next().unwrap(), rec, single_workid, variables)?.trim()
            );
        }
        Rule::match_pattern_fn => {
            //safe
            let pair =  pair.into_inner().next().unwrap();
            let signature_name = if pair.as_rule() == Rule::variable {
                let variable = get_variable(variables, &pair.0)?;
                let VariableValue::ClamPattern { name, .. } = variable else {
                    return Err(incompatible_variable(pair.as_span()));
                };
                name.to_string()
            }
            else {
                let mut inner = pair.into_inner();
                //safe
                let mut pattern_pair = inner.next().unwrap();
                let prefix = if pattern_pair.as_rule() == Rule::clam_offset {
                    let prefix = format!("0:{}:", pattern_pair.as_str());
                    //safe
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
                format!("ContexQL.Pattern.{hash}")
            };
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
        | Rule::variable
        | Rule::variable_bool
        | Rule::variable_number
        | Rule::variable_string
        | Rule::variable_date
        | Rule::variable_clam_pattern
        | Rule::variable_value
        | Rule::variable_definition
        | Rule::jsonpath_object_match
        | Rule::jsonpath_object_match_condition
        | Rule::jsonpath_object_match_equals
        | Rule::jsonpath_object_match_compares
        => unreachable!(),
    }
    Ok(res)
}

fn get_variable<'a>(
    variables: &'a HashMap<String, VariableValue>,
    pair: &Pair<'_, Rule>,
) -> Result<&'a VariableValue, Box<pest::error::Error<Rule>>> {
    variables.get(pair.as_str()).ok_or_else(|| {
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "defined variable".to_string(),
            },
            pair.as_span(),
        )
        .into()
    })
}

fn incompatible_variable(span: pest::Span) -> Box<pest::error::Error<Rule>> {
    pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError {
            message: "compatible variable".to_string(),
        },
        span,
    )
    .into()
}

pub fn parse_to_sql<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let mut parsed = RuleParser::parse(Rule::rule, expr.as_ref())?;
    let parsed = parsed.next().unwrap(); // cannot fail: rule matches from SOI to EOI
    let res = to_sql(PairWrapper(parsed), 0, false);
    debug!("parse_to_sql({}) => {:?}", expr, res);
    res
}

pub fn parse_to_sql_single_work<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let mut parsed = RuleParser::parse(Rule::rule, expr.as_ref())?;
    let parsed = parsed.next().unwrap(); // cannot fail: rule matches from SOI to EOI
    let res = to_sql(PairWrapper(parsed), 0, true);
    debug!("parse_to_sql_single_work({}) => {:?}", expr, res);
    res
}

pub fn parse_and_extract_clam_signatures<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<Vec<String>, Box<pest::error::Error<Rule>>> {
    let mut result = Vec::new();
    let mut parsed = RuleParser::parse(Rule::rule, expr.as_ref())?;
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

fn escape_pair_as_constant_string(
    pair: pest::iterators::Pair<Rule>,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let input = rules::unescape_string(pair)?;
    Ok(escape_string_as_constant_string(&input))
}

fn escape_string_as_constant_string(input: &str) -> String {
    let input = input.replace('\0', "\u{f2b3}");
    postgres_protocol::escape::escape_literal(&input)
}

fn escape_pair_as_json_string(
    pair: pest::iterators::Pair<Rule>,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let input = rules::unescape_string(pair)?.replace('\0', "\u{f2b3}");
    Ok(escape_string_as_json_string(&input))
}

fn escape_string_as_json_string(input: &str) -> String {
    let input = input.replace('\0', "\u{f2b3}");
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
    result
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
