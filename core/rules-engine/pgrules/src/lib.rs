mod code_completion;
mod version;

pub use code_completion::{get_code_completion, Position, Token};
use pest::{iterators::Pair, Parser, Span};
use rules::{unescape_string, Rule, RuleParser};
use std::collections::HashMap;
use tracing::{debug, trace};
use version::RuleVersion;
pub use version::{CURRENT_VERSION, CURRENT_VERSION_STR};

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

impl std::fmt::Debug for PairWrapper<'_> {
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

#[derive(Debug)]
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
    Selector(String),
}

#[derive(Debug, Clone, Copy)]
pub enum QueryType {
    Search,
    ScenarioLocal,
    ScenarioGlobal,
}

#[derive(Debug)]
pub struct Selector {
    pub name: String,
    pub query: String,
}

impl Selector {
    fn to_sql(&self) -> String {
        let name = format!("tmp_{}", &self.name[2..self.name.len() - 1]);
        let name = postgres_protocol::escape::escape_identifier(&name);
        format!("{name} AS ({})", self.query)
    }
}

#[derive(Debug)]
pub struct SqlCommand {
    pub query: String,
    pub with_clause: Option<String>,
    pub global_query_settings: Option<GlobalquerySettings>,
}

#[derive(Default)]
struct ToSqlContext {
    variables: HashMap<String, VariableValue>,
    global_query_settings: Option<GlobalquerySettings>,
}

pub fn to_sql(
    pair: PairWrapper,
    rec: u32,
    query_type: QueryType,
) -> Result<SqlCommand, Box<pest::error::Error<Rule>>> {
    let mut context = ToSqlContext::default();
    let query = to_sql_inner(pair, rec, query_type, &mut context)?;
    let local_selectors = context
        .variables
        .iter()
        .filter_map(|(name, value)| {
            if let VariableValue::Selector(query) = value {
                let name = name.to_string();
                let query = query.to_string();
                Some(Selector { name, query })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let with_clause = if local_selectors.is_empty() {
        None
    } else {
        let mut iter = local_selectors.into_iter();
        //safe
        let mut with_clause = format!("WITH {}", iter.next().unwrap().to_sql());
        for selector in iter {
            with_clause += &format!(", {}", selector.to_sql())
        }
        Some(with_clause)
    };
    Ok(SqlCommand {
        query,
        with_clause,
        global_query_settings: context.global_query_settings,
    })
}

fn parse_variable_definition(
    pair: PairWrapper,
    query_type: QueryType,
    context: &mut ToSqlContext,
) -> Result<(), Box<pest::error::Error<Rule>>> {
    assert!(
        [Rule::variable_definition, Rule::variable_definition_global].contains(&pair.as_rule())
    );
    let mut inner = pair.into_inner();
    //safe
    let name_pair = inner.next().unwrap();
    let name = name_pair.as_str();
    if context.variables.contains_key(name) {
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
    inner = value_pair.into_inner();
    //safe
    let mut value_pair = inner.next().unwrap();
    let value = match value_rule {
        Rule::variable_value_bool => {
            let value = to_sql_inner(value_pair, 1, QueryType::Search, context)?;
            VariableValue::Bool(value)
        }
        Rule::variable_value_number => {
            let value = to_sql_inner(value_pair, 1, QueryType::Search, context)?;
            VariableValue::Number(value)
        }
        Rule::variable_value_string => {
            let value = unescape_string(value_pair.0)?;
            VariableValue::String(value)
        }
        Rule::variable_value_date => {
            let interval = match value_pair.as_rule() {
                Rule::date => "1",
                Rule::datetime => "INTERVAL '1 seconds'",
                _ => unreachable!(),
            }
            .to_string();
            let date_string = to_sql_inner(value_pair, 1, QueryType::Search, context)?;
            VariableValue::DateTime {
                date_string,
                interval,
            }
        }
        Rule::variable_value_clam_pattern => {
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
            VariableValue::ClamPattern { name }
        }
        Rule::variable_value_selector => {
            if !matches!(query_type, QueryType::ScenarioGlobal) {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Selectors can be defined only in global query".to_string(),
                    },
                    value_pair.as_span(),
                )
                .into());
            }
            let (what, from) = {
                let rec = 0;
                let curobj =
                    postgres_protocol::escape::escape_identifier(&format!("objects_{rec}"));
                let mut from = format!("FROM objects AS {0} WHERE {0}.work_id = $2", curobj);
                if value_pair.as_rule() == Rule::variable_value_selector_filter {
                    // safe
                    from += &format!(
                        " AND {}",
                        to_sql_inner(
                            value_pair.into_inner().next().unwrap(),
                            rec,
                            QueryType::Search,
                            context
                        )?
                    );
                    //safe
                    value_pair = inner.next().unwrap();
                }
                //safe
                value_pair = value_pair.into_inner().next().unwrap();
                let what = if value_pair.as_rule() == Rule::get_relation_meta_fn {
                    let path = to_sql_inner(
                        value_pair.into_inner().next().unwrap(),
                        rec,
                        QueryType::Search,
                        context,
                    )?;
                    from += &format!(" AND (EXISTS (SELECT 1 FROM rels WHERE child={curobj}.id and props @? '{path}'))");
                    format!("(select jsonb_path_query(props, '{path}') from rels where child={curobj}.id)")
                } else {
                    to_sql_inner(value_pair, rec, QueryType::Search, context)?
                };
                (what, from)
            };
            let query = format!("SELECT {what} {from}");
            VariableValue::Selector(query)
        }
        _ => unreachable!("rule {:?} => {}", value_pair.as_rule(), value_pair.as_str()),
    };
    context.variables.insert(name.to_string(), value);
    Ok(())
}

fn to_sql_inner(
    pair: PairWrapper,
    rec: u32,
    query_type: QueryType,
    context: &mut ToSqlContext,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let pair = match pair.as_rule() {
        Rule::rule | Rule::rule_global => {
            //safe, extract rule_body
            pair.into_inner().next().unwrap()
        }
        _ => pair,
    };
    let mut res = String::new();
    trace!("Parsing: {}", pair.as_str());
    trace!("Result: {pair:#?}\n\n");
    let curobj = postgres_protocol::escape::escape_identifier(&format!("objects_{rec}"));
    let nextobj = postgres_protocol::escape::escape_identifier(&format!("objects_{}", rec + 1));
    let single_workid = !matches!(query_type, QueryType::Search);
    let match_work = if single_workid {
        format!("{nextobj}.work_id = $1")
    } else {
        format!("{nextobj}.work_id = {curobj}.work_id")
    };
    match pair.as_rule() {
        Rule::rule_body | Rule::rule_body_global => {
            if single_workid {
                res = format!("FROM objects AS {0} WHERE {0}.work_id = $1 AND ", curobj);
            } else {
                res = format!("FROM objects AS {curobj} WHERE ");
            }
            let mut inner = pair.into_inner();
            let mut pair = inner.next().unwrap();
            if pair.as_rule() == Rule::global_query_settings {
                context.global_query_settings = Some(parse_global_query_settings(pair)?);
                pair = inner.next().unwrap();
            }
            let rule_variables = pair;
            let node = inner.next().unwrap();
            for p in rule_variables.into_inner() {
                parse_variable_definition(p, query_type, context)?;
            }
            res += &to_sql_inner(node, rec, query_type, context)?;
        }
        Rule::string
        | Rule::string_raw
        | Rule::string_regular
        | Rule::string_raw_value
        | Rule::string_regular_value => {
            unreachable!("Invalid usage of Rule::string*. Use rules::unescape_string and format/escape string properly to context.");
        }
        Rule::constant_string | Rule::constant_string_object_type => {
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
                res += &to_sql_inner(p, rec, query_type, context)?;
            }
        }
        Rule::cond => {
            #[derive(PartialEq)]
            enum Type {
                Bool,
                Number,
                String,
                Other,
            }
            let mut inner = pair.into_inner();
            // safe
            let left = inner.next().unwrap();
            let expected_type = match left.as_rule() {
                Rule::ident_bool | Rule::functions_bool | Rule::bool => Type::Bool,
                Rule::ident_number | Rule::functions_number => Type::Number,
                Rule::ident_string | Rule::ident_string_object_type | Rule::functions_string => {
                    Type::String
                }
                _ => unreachable!(),
            };
            res += &to_sql_inner(left, rec, query_type, context)?;
            if let Some(pair) = inner.next() {
                if pair.as_rule() == Rule::in_statement_selector {
                    let mut inner = pair.into_inner();
                    //safe
                    let in_operator = inner.next().unwrap();
                    let not_prefix = if in_operator.into_inner().next().is_some() {
                        "NOT "
                    } else {
                        ""
                    };
                    //safe
                    let variable_pair = inner.next().unwrap();
                    let table_name =
                        find_selector_table_name(&mut context.variables, &variable_pair.0)?;
                    res += &format!("{not_prefix} IN (SELECT * FROM {table_name})");
                } else if [
                    Rule::in_statement_string,
                    Rule::in_statement_string_object_type,
                    Rule::in_statement_number,
                ]
                .contains(&pair.as_rule())
                {
                    let mut inner = pair.into_inner();
                    //safe
                    let in_operator = inner.next().unwrap();
                    if in_operator.into_inner().next().is_some() {
                        res += " not";
                    }
                    res += " in (";
                    for (index, pair) in inner.enumerate() {
                        if index != 0 {
                            res += ", ";
                        }
                        if is_variable_rule(pair.as_rule()) {
                            let variable = get_variable(&context.variables, &pair.0)?;
                            let (variable_type, variable_value) = match variable {
                                VariableValue::Bool(v) => (Type::Bool, v.as_str()),
                                VariableValue::Number(v) => (Type::Number, v.as_str()),
                                VariableValue::String(v) => (Type::String, v.as_str()),
                                VariableValue::DateTime { .. }
                                | VariableValue::ClamPattern { .. }
                                | VariableValue::Selector(_) => (Type::Other, ""),
                            };
                            if variable_type != expected_type {
                                return Err(incompatible_variable(pair.as_span()));
                            }
                            if variable_type == Type::String {
                                let escaped_string =
                                    escape_string_as_constant_string(variable_value);
                                res += &escaped_string;
                            } else {
                                res += variable_value;
                            }
                        } else {
                            res += &to_sql_inner(pair, rec, query_type, context)?;
                        }
                    }
                    res += ")";
                } else {
                    res += &to_sql_inner(pair, rec, query_type, context)?;
                    // safe
                    let right = inner.next().unwrap();
                    if is_variable_rule(right.as_rule()) {
                        let variable = get_variable(&context.variables, &right.0)?;
                        let (variable_type, variable_value) = match variable {
                            VariableValue::Bool(v) => (Type::Bool, v.as_str()),
                            VariableValue::Number(v) => (Type::Number, v.as_str()),
                            VariableValue::String(v) => (Type::String, v.as_str()),
                            VariableValue::DateTime { .. }
                            | VariableValue::ClamPattern { .. }
                            | VariableValue::Selector(_) => (Type::Other, ""),
                        };
                        if variable_type != expected_type {
                            return Err(incompatible_variable(right.as_span()));
                        }
                        if variable_type == Type::String {
                            let escaped_string = escape_string_as_constant_string(variable_value);
                            res += &escaped_string;
                        } else {
                            res += variable_value;
                        }
                    } else {
                        res += &to_sql_inner(right, rec, query_type, context)?;
                    }
                }
            }
        }
        Rule::node => {
            res += "(";
            for p in pair.into_inner() {
                res += &to_sql_inner(p, rec, query_type, context)?;
            }
            res += ")";
        }
        Rule::ident_bool
        | Rule::is_root_fn
        | Rule::ident_number
        | Rule::ident_string
        | Rule::ident_string_object_type => {
            let ident = match pair.as_str() {
                "is_root()" => "is_entry",
                other => other,
            };
            res += &format!(
                "{curobj}.{}",
                postgres_protocol::escape::escape_identifier(ident)
            );
        }
        Rule::bool
        | Rule::integer
        | Rule::number
        | Rule::compares
        | Rule::unsigned_integer
        | Rule::gqs_max_neighbors_value => res += pair.as_str(),
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
            let mut inner = pair.into_inner();
            let pair = inner.0.peek().unwrap();
            if pair.as_rule() == Rule::in_statement_selector {
                let mut inner = pair.into_inner();
                //safe
                let in_operator = inner.next().unwrap();
                let not_prefix = if in_operator.into_inner().next().is_some() {
                    "NOT "
                } else {
                    ""
                };
                //safe
                let variable_pair = inner.next().unwrap();
                let table_name = find_selector_table_name(&mut context.variables, &variable_pair)?;
                let command = format!(
                    r#"{not_prefix}EXISTS (SELECT 1 FROM objects AS {nextobj}, jsonb_array_elements_text(result->'ok'->'symbols') AS symbol WHERE {curobj}.id={nextobj}.id AND symbol IN (SELECT * FROM {table_name}))"#
                );
                res += &command;
            } else {
                if pair.as_rule() == Rule::in_statement_string_symbol {
                    inner = PairsWrapper(pair.into_inner());
                }
                let mut conditions = Vec::new();
                for argument in inner {
                    let condition = match argument.as_rule() {
                        Rule::string_symbol => format!(
                            "{curobj}.\"result\"->'ok'->'symbols'?{}",
                            escape_pair_as_constant_string(argument.0)?
                        ),
                        Rule::variable_bool
                        | Rule::variable_clam_pattern
                        | Rule::variable_date
                        | Rule::variable_json
                        | Rule::variable_number
                        | Rule::variable_selector
                        | Rule::variable_string => {
                            let variable = get_variable(&context.variables, &argument.0)?;
                            let VariableValue::String(variable_value) = variable else {
                                return Err(incompatible_variable(argument.as_span()));
                            };
                            format!(
                                "{curobj}.\"result\"->'ok'->'symbols'?{}",
                                escape_string_as_constant_string(variable_value)
                            )
                        }
                        Rule::func_arg_regex
                        | Rule::func_arg_iregex
                        | Rule::func_arg_starts_with => {
                            format!(
                                "{curobj}.\"result\"->'ok'->'symbols'@?'$?(@{})'",
                                to_sql_inner(argument, rec, query_type, context)?
                            )
                        }
                        _ => unreachable!(),
                    };
                    conditions.push(condition);
                }
                if conditions.len() == 1 {
                    res += conditions.first().unwrap();
                } else {
                    let condition = format!("({})", conditions.join(" OR "));
                    res += &condition;
                }
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
                    Rule::variable_bool
                    | Rule::variable_clam_pattern
                    | Rule::variable_date
                    | Rule::variable_json
                    | Rule::variable_number
                    | Rule::variable_selector
                    | Rule::variable_string => {
                        let variable = get_variable(&context.variables, &argument.0)?;
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
                            to_sql_inner(argument, rec, query_type, context)?
                        );
                    }
                    _ => unreachable!(),
                }
            } else {
                res += &format!("{curobj}.\"result\"?'error'");
            }
        }
        Rule::has_name_fn => {
            let mut inner = pair.into_inner();
            let pair = inner.0.peek().unwrap();
            if pair.as_rule() == Rule::in_statement_selector {
                let mut inner = pair.into_inner();
                //safe
                let in_operator = inner.next().unwrap();
                let not_prefix = if in_operator.into_inner().next().is_some() {
                    "NOT "
                } else {
                    ""
                };
                //safe
                let variable_pair = inner.next().unwrap();
                let table_name = find_selector_table_name(&mut context.variables, &variable_pair)?;
                let from_command = ["props->'name'", "jsonb_path_query(props, '$.names[*]')"]
                    .map(|s| format!("SELECT jsonb_array_elements_text(json_array(SELECT {s} FROM rels WHERE child={curobj}.id)) AS name"))
                    .join(" UNION ");
                let command = format!(
                    r#"{not_prefix}EXISTS (SELECT * FROM ({from_command}) WHERE name IN (SELECT * FROM {table_name}))"#
                );
                res += &command;
            } else {
                if pair.as_rule() == Rule::in_statement_string_extended {
                    inner = PairsWrapper(pair.into_inner());
                }
                let mut conditions = Vec::new();
                for argument in inner {
                    let condition = match argument.as_rule() {
                        Rule::string => {
                            format!("=={}", escape_pair_as_json_string(argument.0)?)
                        }
                        Rule::variable_bool
                        | Rule::variable_clam_pattern
                        | Rule::variable_date
                        | Rule::variable_json
                        | Rule::variable_number
                        | Rule::variable_selector
                        | Rule::variable_string => {
                            let variable = get_variable(&context.variables, &argument.0)?;
                            let VariableValue::String(variable_value) = variable else {
                                return Err(incompatible_variable(argument.as_span()));
                            };
                            format!("=={}", escape_string_as_json_string(variable_value))
                        }
                        Rule::func_arg_regex
                        | Rule::func_arg_iregex
                        | Rule::func_arg_starts_with => {
                            to_sql_inner(argument, rec, query_type, context)?
                        }
                        _ => unreachable!(),
                    };
                    conditions.push(format!("@.name{condition} || @.names[*]{condition}"));
                }
                let condition = conditions.join(" || ");
                res += &format!(
                    "exists(SELECT 1 FROM rels WHERE child = {curobj}.\"id\" AND (props @? '$ ? ({condition})'))",
                );
            }
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
                Some(p) => format!(" AND {}", to_sql_inner(p, rec + 1, query_type, context)?),
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
                Some(p) => format!(" AND {}", to_sql_inner(p, rec + 1, query_type, context)?),
                None => String::new(),
            };
            res += &format!(
                "{header} FROM objects AS {nextobj} WHERE {match_work} AND id IN (SELECT child FROM rels WHERE parent = {curobj}.\"id\"){node_def})"
            );
        }
        Rule::has_root_fn => {
            res += &format!(
                "exists(SELECT 1 FROM objects AS {nextobj} WHERE {match_work} AND is_entry AND {})",
                to_sql_inner(
                    pair.into_inner().next().unwrap(),
                    rec + 1,
                    query_type,
                    context
                )?
            );
        }
        Rule::has_parent_fn => {
            res += &format!(
                "exists(SELECT 1 FROM objects AS {nextobj} WHERE {match_work} AND id = (SELECT parent FROM rels WHERE child = {curobj}.\"id\") AND {})",
                to_sql_inner(pair.into_inner().next().unwrap(), rec + 1, query_type,  context)?
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
                Some(p) => format!(" AND {}", to_sql_inner(p, rec + 1, query_type, context)?),
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
            res += &to_sql_inner(inner.next().unwrap(), rec + 1, query_type, context)?;
            res += " ";
            res += &to_sql_inner(inner.next().unwrap(), rec + 1, query_type, context)?;
        }
        Rule::date_range_fn => {
            let mut inner = pair.into_inner();
            //safe
            let start_pair = inner.next().unwrap();
            let start = if is_variable_rule(start_pair.as_rule()) {
                let variable = get_variable(&context.variables, &start_pair.0)?;
                let VariableValue::DateTime { date_string, .. } = variable else {
                    return Err(incompatible_variable(start_pair.as_span()));
                };
                date_string.to_string()
            } else {
                to_sql_inner(start_pair, rec + 1, query_type, context)?
            };
            //safe
            let end_pair = inner.next().unwrap();
            let (end, interval) = if is_variable_rule(end_pair.as_rule()) {
                let variable = get_variable(&context.variables, &end_pair.0)?;
                let VariableValue::DateTime {
                    date_string,
                    interval,
                } = variable
                else {
                    return Err(incompatible_variable(end_pair.as_span()));
                };
                (date_string.to_string(), interval.as_str())
            } else {
                let interval = match end_pair.as_rule() {
                    Rule::date => "1",
                    Rule::datetime => "INTERVAL '1 seconds'",
                    _ => unreachable!(),
                };
                let end = to_sql_inner(end_pair, rec + 1, query_type, context)?;
                (end, interval)
            };
            res += &format!(
                "{curobj}.t BETWEEN '{start}' AND (DATE '{end}'+{interval}-INTERVAL '1 microseconds')"
            );
        }
        Rule::date_since_fn => {
            //safe
            let pair = pair.into_inner().next().unwrap();
            let start = if is_variable_rule(pair.as_rule()) {
                let variable = get_variable(&context.variables, &pair.0)?;
                let VariableValue::DateTime { date_string, .. } = variable else {
                    return Err(incompatible_variable(pair.as_span()));
                };
                date_string.to_string()
            } else {
                to_sql_inner(pair, rec + 1, query_type, context)?
            };
            res += &format!("{curobj}.t >= '{start}'",);
        }
        Rule::match_object_meta_fn
        | Rule::has_object_meta_fn
        | Rule::match_relation_meta_fn
        | Rule::has_relation_meta_fn => {
            res += &parse_meta_fn_pair(pair, rec, query_type, context, &curobj, &nextobj)?;
        }
        Rule::func_arg_regex | Rule::func_arg_iregex => {
            let flag = match pair.as_rule() {
                Rule::func_arg_regex => "",
                Rule::func_arg_iregex => " flag \"i\"",
                _ => unreachable!(),
            };
            //safe
            let pair = pair.into_inner().next().unwrap();
            if is_variable_rule(pair.as_rule()) {
                let variable = get_variable(&context.variables, &pair.0)?;
                let VariableValue::String(variable_value) = variable else {
                    return Err(incompatible_variable(pair.as_span()));
                };
                let regex = escape_string_as_json_string(variable_value);
                res += &format!(" like_regex {regex}{flag}");
            } else {
                let regex = escape_pair_as_json_string(pair.0)?;
                res += &format!(" like_regex {regex}{flag}");
            }
        }
        Rule::func_arg_starts_with => {
            //safe
            let pair = pair.into_inner().next().unwrap();
            if is_variable_rule(pair.as_rule()) {
                let variable = get_variable(&context.variables, &pair.0)?;
                let VariableValue::String(variable_value) = variable else {
                    return Err(incompatible_variable(pair.as_span()));
                };
                let prefix = escape_string_as_json_string(variable_value);
                res += &format!(" starts with {prefix}");
            } else {
                let prefix = escape_pair_as_json_string(pair.0)?;
                res += &format!(" starts with {prefix}");
            }
        }
        Rule::jsonpath_path_simple => {
            res += "$";
            for p in pair.into_inner() {
                res += &to_sql_inner(p, rec, query_type, context)?;
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
                    }
                    .to_string()
                } else {
                    to_sql_inner(p, rec, query_type, context)?
                };
                res += &v;
            }
            res += ")";
        }
        Rule::jsonpath_object_match_condition_simple => {
            let mut inner = pair.into_inner();
            //safe
            let id = to_sql_inner(inner.next().unwrap(), rec, query_type, context)?;
            //safe
            let op_pair = inner.next().unwrap();
            match op_pair.as_rule() {
                Rule::jsonpath_object_match_equals | Rule::jsonpath_object_match_compares => {
                    let mut inner = op_pair.into_inner();
                    //safe
                    let op_pair = inner.next().unwrap();
                    let op = op_pair.as_str();
                    let (op, negate) = {
                        if ["!=", "<>"].contains(&op) {
                            ("!=", true)
                        } else {
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
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let val = match val_pair.as_rule() {
                        Rule::variable_bool
                        | Rule::variable_clam_pattern
                        | Rule::variable_date
                        | Rule::variable_json
                        | Rule::variable_number
                        | Rule::variable_selector
                        | Rule::variable_string => {
                            let variable = get_variable(&context.variables, &val_pair.0)?;
                            if op_pair.as_rule() == Rule::jsonpath_object_match_compares {
                                let VariableValue::Number(val) = variable else {
                                    return Err(incompatible_variable(val_pair.as_span()));
                                };
                                val.to_string()
                            } else {
                                if negate {
                                    negate_type = match variable {
                                        VariableValue::String(_) => Some("string"),
                                        VariableValue::Number(_) => Some("number"),
                                        VariableValue::Bool(_) => Some("boolean"),
                                        _ => None,
                                    };
                                }
                                match variable {
                                    VariableValue::Bool(v) => v.to_string(),
                                    VariableValue::Number(v) => v.to_string(),
                                    VariableValue::String(v) => escape_string_as_json_string(v),
                                    VariableValue::DateTime { .. }
                                    | VariableValue::ClamPattern { .. }
                                    | VariableValue::Selector(_) => {
                                        return Err(incompatible_variable(val_pair.as_span()))
                                    }
                                }
                            }
                        }
                        Rule::string => escape_pair_as_json_string(val_pair.0)?,
                        _ => to_sql_inner(val_pair, rec, query_type, context)?,
                    };
                    if let Some(negate_type) = negate_type {
                        res += &format!(r#"({id}{op}{val} || {id}.type()!="{negate_type}")"#)
                    } else {
                        res += &format!("{id}{op}{val}")
                    }
                }
                Rule::func_arg_regex | Rule::func_arg_iregex | Rule::func_arg_starts_with => {
                    let func = to_sql_inner(op_pair, rec, query_type, context)?;
                    res += &format!("{id}{func}")
                }
                Rule::in_statement_jsonpath_object => {
                    let mut conditions = Vec::new();
                    for val_pair in op_pair.into_inner() {
                        let operator = if [
                            Rule::bool,
                            Rule::number,
                            Rule::string,
                            Rule::variable_json,
                            Rule::jsonpath_object_match_id,
                        ]
                        .contains(&val_pair.as_rule())
                        {
                            "=="
                        } else {
                            ""
                        };
                        let val = match val_pair.as_rule() {
                            Rule::variable_bool
                            | Rule::variable_clam_pattern
                            | Rule::variable_date
                            | Rule::variable_json
                            | Rule::variable_number
                            | Rule::variable_selector
                            | Rule::variable_string => {
                                let variable = get_variable(&context.variables, &val_pair.0)?;
                                match variable {
                                    VariableValue::Bool(v) => v.to_string(),
                                    VariableValue::Number(v) => v.to_string(),
                                    VariableValue::String(v) => escape_string_as_json_string(v),
                                    VariableValue::DateTime { .. }
                                    | VariableValue::ClamPattern { .. }
                                    | VariableValue::Selector(_) => {
                                        return Err(incompatible_variable(val_pair.as_span()))
                                    }
                                }
                            }
                            Rule::string => escape_pair_as_json_string(val_pair.0)?,
                            _ => to_sql_inner(val_pair, rec, query_type, context)?,
                        };
                        conditions.push(format!("{id}{operator}{val}"));
                    }
                    if conditions.len() == 1 {
                        res += conditions.first().unwrap();
                    } else {
                        res += &format!("({})", conditions.join(" || "));
                    }
                }
                _ => unreachable!(),
            };
        }
        Rule::jsonpath_object_match_id => {
            res += "@";
            //safe
            res += &to_sql_inner(pair.into_inner().next().unwrap(), rec, query_type, context)?;
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
            res += &to_sql_inner(inner.next().unwrap(), rec, query_type, context)?;
        }
        Rule::jsonpath_selector_index => {
            let mut inner = pair.into_inner();
            res += &format!(
                "[{}]",
                to_sql_inner(inner.next().unwrap(), rec, query_type, context)?.trim()
            );
        }
        Rule::match_pattern_fn => {
            //safe
            let pair = pair.into_inner().next().unwrap();
            let signature_name = if is_variable_rule(pair.as_rule()) {
                let variable = get_variable(&context.variables, &pair.0)?;
                let VariableValue::ClamPattern { name, .. } = variable else {
                    return Err(incompatible_variable(pair.as_span()));
                };
                name.to_string()
            } else {
                let mut inner = pair.into_inner();
                //safe
                let mut pattern_pair = inner.next().unwrap();
                let prefix = if pattern_pair.as_rule() == Rule::clam_offset {
                    let prefix = format!("0:{}:", pattern_pair.as_str());
                    //safe
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
                format!("ContexQL.Pattern.{hash}")
            };
            res += &format!(
                "{curobj}.\"result\"->'ok'->'symbols'?{}",
                postgres_protocol::escape::escape_literal(&signature_name)
            );
        }
        Rule::count_conditions_fn => {
            res += "(SELECT ";
            let mut inner = pair.into_inner();
            //safe
            let pair = inner.next().unwrap();
            res += &format!(
                "CASE WHEN {} THEN 1 ELSE 0 END",
                to_sql_inner(pair, rec + 1, query_type, context)?
            );
            for pair in inner {
                res += &format!(
                    " + CASE WHEN {} THEN 1 ELSE 0 END",
                    to_sql_inner(pair, rec + 1, query_type, context)?
                );
            }
            res += &format!(" FROM objects AS {nextobj} WHERE {curobj}.id={nextobj}.id)");
        }
        Rule::get_symbols_fn => {
            res += &format!(r#"jsonb_array_elements_text({curobj}."result"->'ok'->'symbols')"#);
        }
        Rule::get_names_fn => {
            let command = ["props->'name'", "jsonb_path_query(props, '$.names[*]')"]
                    .map(|s| format!("SELECT jsonb_array_elements_text(json_array(SELECT {s} FROM rels WHERE child={curobj}.id)) AS name"))
                    .join(" UNION ");
            res += &format!("({command})");
        }
        Rule::get_object_meta_fn => {
            let mut inner = pair.into_inner();
            let path = &to_sql_inner(inner.next().unwrap(), rec, query_type, context)?;
            res += &format!(r#"jsonb_path_query(result->'ok'->'object_metadata', '{path}')"#);
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
        | Rule::variable_value_bool
        | Rule::variable_value_number
        | Rule::variable_value_string
        | Rule::variable_value_date
        | Rule::variable_value_clam_pattern
        | Rule::variable_value
        | Rule::variable_value_global
        | Rule::variable_definition
        | Rule::variable_definition_global
        | Rule::jsonpath_object_match
        | Rule::jsonpath_object_match_condition
        | Rule::jsonpath_object_match_equals
        | Rule::jsonpath_object_match_compares
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
        | Rule::get_relation_meta_fn
        | Rule::in_statement_selector
        | Rule::string_symbol
        | Rule::variable_bool
        | Rule::variable_number
        | Rule::variable_string
        | Rule::variable_date
        | Rule::variable_clam_pattern
        | Rule::variable_json
        | Rule::variable_selector
        | Rule::rule
        | Rule::rule_global
        | Rule::rule_variables
        | Rule::rule_variables_global
        | Rule::rule_body_partial
        | Rule::rule_body_partial_global
        | Rule::global_query_settings
        | Rule::global_query_setting
        | Rule::gqs_matches
        | Rule::gqs_matches_value
        | Rule::gqs_time_window
        | Rule::gqs_time_window_value
        | Rule::gqs_time_window_unit
        | Rule::gqs_max_neighbors
        | Rule::variable => unreachable!(),
    }
    Ok(res)
}

fn new_pest_error<T: ToString>(message: T, span: pest::Span) -> pest::error::Error<Rule> {
    pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError {
            message: message.to_string(),
        },
        span,
    )
}

fn parse_global_query_settings(
    pair: PairWrapper,
) -> Result<GlobalquerySettings, Box<pest::error::Error<Rule>>> {
    let span = pair.as_span();
    let inner = PairsWrapper(pair.0.clone().into_inner());
    let mut matches: Option<Matches> = None;
    let mut time_window: Option<std::time::Duration> = None;
    let mut max_neighbors: Option<u32> = None;

    for pair in inner {
        match pair.as_rule() {
            Rule::gqs_matches => {
                if matches.is_some() {
                    return Err(new_pest_error("MATCHES is already defined", pair.as_span()).into());
                }
                //safe
                let value_pair = pair.into_inner().next().unwrap();
                matches = Some(Matches::from_pair(value_pair.0)?);
            }
            Rule::gqs_time_window => {
                if time_window.is_some() {
                    return Err(
                        new_pest_error("TIME_WINDOW is already defined", pair.as_span()).into(),
                    );
                }
                //safe
                let span = pair.as_span();
                let value_pair = PairsWrapper(pair.0.clone().into_inner()).next().unwrap();
                let value_str = value_pair.as_str();
                let duration = match humantime::parse_duration(value_str) {
                    Ok(duration) => duration,
                    Err(err) => return Err(new_pest_error(err, span).into()),
                };
                time_window = Some(duration);
            }
            Rule::gqs_max_neighbors => {
                if max_neighbors.is_some() {
                    return Err(
                        new_pest_error("MAX_NEIGHBORS is already defined", pair.as_span()).into(),
                    );
                }
                let value_pair = PairsWrapper(pair.0.clone().into_inner()).next().unwrap();
                let value_str = value_pair.as_str();
                //safe
                let number = value_str.parse().unwrap();
                max_neighbors = Some(number);
            }
            _ => unreachable!(),
        }
    }

    let matches = matches.ok_or_else(|| new_pest_error("MATCHES is not defined", span))?;
    let time_window =
        time_window.ok_or_else(|| new_pest_error("TIME_WINDOW is not defined", span))?;
    let time_window = Interval::try_from(time_window)
        .map_err(|e| new_pest_error(format!("Invalid TIME_WINDOW: {e}"), span))?;
    Ok(GlobalquerySettings {
        matches,
        time_window,
        max_neighbors,
    })
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

fn find_selector_table_name(
    variables: &mut HashMap<String, VariableValue>,
    pair: &Pair<'_, Rule>,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let variable = get_variable(variables, pair)?;
    if !matches!(variable, VariableValue::Selector(_)) {
        return Err(incompatible_variable(pair.as_span()));
    }
    //safe
    let name = pair.as_str();
    let name = &name[2..name.len() - 1];
    let table_name = postgres_protocol::escape::escape_identifier(&format!("tmp_{}", name));
    Ok(table_name)
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
    sql_context: QueryType,
) -> Result<SqlCommand, Box<pest::error::Error<Rule>>> {
    let r = match sql_context {
        QueryType::ScenarioGlobal => Rule::rule_global,
        _ => Rule::rule,
    };
    let mut parsed = RuleParser::parse(r, expr.as_ref()).map_err(modify_pest_error)?;
    let parsed = parsed.next().unwrap(); // cannot fail: rule matches from SOI to EOI
    let res = to_sql(PairWrapper(parsed), 0, sql_context);
    debug!("parse_to_sql({sql_context:?}, {}) => {:?}", expr, res);
    res
}

pub fn detect_query_version<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<RuleVersion, Box<pest::error::Error<Rule>>> {
    let parsed = RuleParser::parse(Rule::rule_global, expr.as_ref()).map_err(modify_pest_error)?;
    Ok(RuleVersion::from_pairs(parsed))
}

pub fn parse_and_extract_clam_signatures<S: AsRef<str> + std::fmt::Display>(
    expr: S,
) -> Result<Vec<String>, Box<pest::error::Error<Rule>>> {
    let mut result = Vec::new();
    let mut parsed =
        RuleParser::parse(Rule::rule_global, expr.as_ref()).map_err(modify_pest_error)?;
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

fn parse_meta_fn_pair(
    pair: PairWrapper,
    rec: u32,
    sql_context: QueryType,
    context: &mut ToSqlContext,
    curobj: &str,
    nextobj: &str,
) -> Result<String, Box<pest::error::Error<Rule>>> {
    let (object_meta, check_condition) = match pair.as_rule() {
        Rule::match_object_meta_fn => (true, true),
        Rule::match_relation_meta_fn => (false, true),
        Rule::has_object_meta_fn => (true, false),
        Rule::has_relation_meta_fn => (false, false),
        _ => unreachable!(),
    };
    let mut inner = pair.into_inner();
    let path = &to_sql_inner(inner.next().unwrap(), rec, sql_context, context)?;

    let mut negate_query = false;
    let (jsonpath, match_length): (String, Option<String>) = if check_condition {
        let pair = inner.next().unwrap();
        if pair.as_rule() == Rule::in_statement_selector {
            let mut inner = pair.into_inner();
            //safe
            let in_operator = inner.next().unwrap();
            let not_prefix = if in_operator.into_inner().next().is_some() {
                "NOT "
            } else {
                ""
            };
            //safe
            let variable_pair = inner.next().unwrap();
            let table_name = find_selector_table_name(&mut context.variables, &variable_pair.0)?;
            let from_command = if object_meta {
                format!(
                    r#"(SELECT jsonb_path_query(result->'ok'->'object_metadata', '{path}') AS meta FROM objects AS {nextobj} WHERE {curobj}.id={nextobj}.id)"#
                )
            } else {
                format!(
                    r#"(SELECT meta FROM rels, jsonb_path_query(props, '$.x') AS meta WHERE child={curobj}.id)"#
                )
            };
            let command = format!(
                r#"{not_prefix}exists(SELECT 1 FROM {from_command} WHERE meta IN (SELECT * FROM {table_name}))"#
            );
            return Ok(command);
        } else if pair.as_rule() == Rule::jsonpath_match_length {
            let jsonpath = format!(
                r#"$.ok.object_metadata{} ? (@.type() == "string")"#,
                &path[1..]
            );
            let inner = pair.into_inner();
            let mut match_length = String::new();
            for pair in inner {
                match_length += " ";
                if is_variable_rule(pair.as_rule()) {
                    let variable = get_variable(&context.variables, &pair.0)?;
                    let VariableValue::Number(variable_value) = variable else {
                        return Err(incompatible_variable(pair.as_span()));
                    };
                    if variable_value.starts_with('-') {
                        return Err(incompatible_variable(pair.as_span()));
                    }
                    match_length += variable_value;
                } else {
                    match_length += &to_sql_inner(pair, rec + 1, sql_context, context)?;
                }
            }
            (jsonpath, Some(match_length))
        } else if pair.as_rule() == Rule::jsonpath_object_match {
            //safe
            let pair = pair.into_inner().next().unwrap();
            let node = to_sql_inner(pair, rec, sql_context, context)?;
            (format!("{path} ? (@!=null && {node})"), None)
        } else if pair.as_rule() == Rule::in_statement_jsonpath {
            let inner = pair.into_inner();
            let mut conditions = Vec::new();
            for pair in inner {
                let operator = if [
                    Rule::bool,
                    Rule::number,
                    Rule::string,
                    Rule::variable_json,
                    Rule::jsonpath_path_simple,
                ]
                .contains(&pair.as_rule())
                {
                    "=="
                } else {
                    ""
                };
                let value = match pair.as_rule() {
                    Rule::string => escape_pair_as_json_string(pair.0)?,
                    Rule::variable_bool
                    | Rule::variable_clam_pattern
                    | Rule::variable_date
                    | Rule::variable_json
                    | Rule::variable_number
                    | Rule::variable_selector
                    | Rule::variable_string => {
                        let variable = get_variable(&context.variables, &pair.0)?;
                        match variable {
                            VariableValue::Bool(v) => v.to_string(),
                            VariableValue::Number(v) => v.to_string(),
                            VariableValue::String(v) => escape_string_as_json_string(v),
                            _ => {
                                return Err(incompatible_variable(pair.as_span()));
                            }
                        }
                    }
                    _ => to_sql_inner(pair, rec, sql_context, context)?,
                };
                conditions.push(format!("@{operator}{value}"));
            }
            let condition: String = if conditions.len() == 1 {
                conditions.first().unwrap().to_string()
            } else {
                format!("({})", conditions.join(" || "))
            };
            let condition = format!("{path} ? (@!=null && {condition})");
            (condition, None)
        } else {
            let comparison = pair.as_rule() == Rule::compares;
            let mut operator = to_sql_inner(pair, rec, sql_context, context)?;
            if ["!=", "<>"].contains(&operator.as_str()) {
                operator = "==".to_string();
                negate_query = true;
            } else if operator == "=" {
                operator = "==".to_string();
            }
            let mut compare_two_variables = false;
            let value = match inner.next() {
                Some(pair) => {
                    let r = pair.as_rule();
                    compare_two_variables = r == Rule::jsonpath_path_simple;
                    match r {
                        Rule::string => escape_pair_as_json_string(pair.0)?,
                        Rule::variable_bool
                        | Rule::variable_clam_pattern
                        | Rule::variable_date
                        | Rule::variable_json
                        | Rule::variable_number
                        | Rule::variable_selector
                        | Rule::variable_string => {
                            let variable = get_variable(&context.variables, &pair.0)?;
                            match variable {
                                VariableValue::Bool(v) if !comparison => v.to_string(),
                                VariableValue::Number(v) => v.to_string(),
                                VariableValue::String(v) if !comparison => {
                                    escape_string_as_json_string(v)
                                }
                                _ => {
                                    return Err(incompatible_variable(pair.as_span()));
                                }
                            }
                        }
                        _ => to_sql_inner(pair, rec, sql_context, context)?,
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
    let mut res = String::new();
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
    Ok(res)
}

fn is_variable_rule(r: Rule) -> bool {
    [
        Rule::variable_bool,
        Rule::variable_clam_pattern,
        Rule::variable_date,
        Rule::variable_json,
        Rule::variable_number,
        Rule::variable_selector,
        Rule::variable_string,
    ]
    .contains(&r)
}

#[derive(Debug)]
pub struct GlobalquerySettings {
    pub matches: Matches,
    pub time_window: Interval,
    pub max_neighbors: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub enum Matches {
    None,
    MoreThan(u32),
    LessThan(u32),
    MoreThanPercent(u32),
    LessThanPercent(u32),
}

impl Matches {
    fn from_pair(pair: Pair<Rule>) -> Result<Self, Box<pest::error::Error<Rule>>> {
        if pair.as_rule() != Rule::gqs_matches_value {
            return Err(new_pest_error("Invalid rule", pair.as_span()).into());
        }
        let mut str = pair.as_str();
        if str == "NONE" {
            return Ok(Self::None);
        }
        let more_than = if str.starts_with('>') {
            true
        } else if str.starts_with('<') {
            false
        } else {
            return Err(new_pest_error("Invalid value", pair.as_span()).into());
        };
        str = &str[1..];
        let percent = str.ends_with('%');
        if percent {
            str = &str[0..str.len() - 1];
        }
        str = str.trim();
        let Ok(value) = str.parse::<u32>() else {
            return Err(new_pest_error("Invalid value", pair.as_span()).into());
        };
        if percent {
            let max = if more_than { 99 } else { 100 };
            if value > max {
                return Err(new_pest_error("Invalid value", pair.as_span()).into());
            }
        }
        let result = match (more_than, percent) {
            (true, true) => Self::MoreThanPercent(value),
            (true, false) => Self::MoreThan(value),
            (false, true) => Self::LessThanPercent(value),
            (false, false) => Self::LessThan(value),
        };
        Ok(result)
    }
}

impl std::fmt::Display for Matches {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Matches::None => write!(f, "NONE"),
            Matches::MoreThan(v) => write!(f, ">{v}"),
            Matches::LessThan(v) => write!(f, "<{v}"),
            Matches::MoreThanPercent(v) => write!(f, ">{v}%"),
            Matches::LessThanPercent(v) => write!(f, "<{v}%"),
        }
    }
}

#[derive(Debug, PartialEq)]
/// PostgreSQL interval (not supported by the tokio_postgres crate)
pub struct Interval {
    micros: i64,
    days: i32,
    months: i32,
}

#[cfg(feature = "interval")]
impl<'a> postgres_types::FromSql<'a> for Interval {
    fn from_sql(
        _ty: &postgres_types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() == 8 + 4 + 4 {
            Ok(Self {
                micros: i64::from_be_bytes(raw[0..8].try_into().unwrap()),
                days: i32::from_be_bytes(raw[8..12].try_into().unwrap()),
                months: i32::from_be_bytes(raw[12..16].try_into().unwrap()),
            })
        } else {
            Err("Failed to deserialize interval value".into())
        }
    }

    fn accepts(ty: &postgres_types::Type) -> bool {
        *ty == postgres_types::Type::INTERVAL
    }
}

#[cfg(feature = "interval")]
impl postgres_types::ToSql for Interval {
    fn to_sql(
        &self,
        _ty: &postgres_types::Type,
        out: &mut bytes::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        use bytes::BufMut as _;
        out.put_i64(self.micros);
        out.put_i32(self.days);
        out.put_i32(self.months);
        Ok(postgres_types::IsNull::No)
    }

    fn accepts(ty: &postgres_types::Type) -> bool {
        *ty == postgres_types::Type::INTERVAL
    }

    postgres_types::to_sql_checked!();
}

impl PartialOrd for Interval {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.days != other.days || self.months != other.months {
            None
        } else {
            Some(self.micros.cmp(&other.micros))
        }
    }
}

impl TryFrom<std::time::Duration> for Interval {
    type Error = std::num::TryFromIntError;

    fn try_from(duration: std::time::Duration) -> Result<Self, Self::Error> {
        Ok(Self {
            micros: i64::try_from(duration.as_micros())?,
            days: 0,
            months: 0,
        })
    }
}
