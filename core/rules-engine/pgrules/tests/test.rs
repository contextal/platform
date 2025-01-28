use pest::Parser;
use pgrules::parse_and_extract_clam_signatures;
use pgrules::parse_to_sql;
use rules::Rule;
use rules::RuleParser;
use std::error::Error;

#[cfg(test)]
fn parse_rule(r: rules::Rule, input: &str) -> Result<String, Box<dyn Error>> {
    use pgrules::{to_sql, PairWrapper};

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

#[test]
fn test_variables() {
    assert_eq!(
        parse_to_sql("${x}=true; is_entry==${x}").unwrap(),
        parse_to_sql("is_entry==true").unwrap()
    );
    assert_eq!(
        parse_to_sql("${x}=1; size==${x}").unwrap(),
        parse_to_sql("size==1").unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; object_type==${x}"#).unwrap(),
        parse_to_sql(r#"object_type=="Text""#).unwrap()
    );
    assert_eq!(
        parse_to_sql("${x}=true; @match_object_meta($x==${x})").unwrap(),
        parse_to_sql("@match_object_meta($x==true)").unwrap()
    );
    assert_eq!(
        parse_to_sql("${x}=1; @match_object_meta($x==${x})").unwrap(),
        parse_to_sql("@match_object_meta($x==1)").unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @match_object_meta($x==${x})"#).unwrap(),
        parse_to_sql(r#"@match_object_meta($x=="Text")"#).unwrap()
    );
    assert!(parse_to_sql("${x}=true; @match_object_meta($x>${x})").is_err());
    assert!(parse_to_sql(r#"${x}="Text"; @match_object_meta($x>${x})"#).is_err());
    assert_eq!(
        parse_to_sql("${x}=1; @match_object_meta($x>${x})"),
        parse_to_sql("@match_object_meta($x>1)")
    );
    assert!(parse_to_sql(r#"${x}=-1; @match_object_meta($x.len()==${x})"#).is_err());
    assert_eq!(
        parse_to_sql("${x}=1; @match_object_meta($x.len()==${x})"),
        parse_to_sql("@match_object_meta($x.len()==1)")
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @match_object_meta($x regex(${x}))"#).unwrap(),
        parse_to_sql(r#"@match_object_meta($x regex("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @match_object_meta($x iregex(${x}))"#).unwrap(),
        parse_to_sql(r#"@match_object_meta($x iregex("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @match_object_meta($x starts_with(${x}))"#).unwrap(),
        parse_to_sql(r#"@match_object_meta($x starts_with("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql("${x}=pattern(deadbeef); @match_pattern(${x})").unwrap(),
        parse_to_sql("@match_pattern(deadbeef)").unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}=datetime("2024-01-01 12:30:00"); @date_since(${x})"#).unwrap(),
        parse_to_sql(r#"@date_since("2024-01-01 12:30:00")"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(
            r#"
            ${x}=datetime("2024-01-01");
            ${y}=datetime("2024-12-31");
            @date_range(${x},${y})
            "#
        )
        .unwrap(),
        parse_to_sql(r#"@date_range("2024-01-01","2024-12-31")"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_symbol(${x})"#).unwrap(),
        parse_to_sql(r#"@has_symbol("Text")"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_symbol(regex(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_symbol(regex("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_symbol(iregex(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_symbol(iregex("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_symbol(starts_with(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_symbol(starts_with("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_name(${x})"#).unwrap(),
        parse_to_sql(r#"@has_name("Text")"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_name(regex(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_name(regex("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_name(iregex(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_name(iregex("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_name(starts_with(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_name(starts_with("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_error(${x})"#).unwrap(),
        parse_to_sql(r#"@has_error("Text")"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_error(regex(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_error(regex("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_error(iregex(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_error(iregex("Text"))"#).unwrap()
    );
    assert_eq!(
        parse_to_sql(r#"${x}="Text"; @has_error(starts_with(${x}))"#).unwrap(),
        parse_to_sql(r#"@has_error(starts_with("Text"))"#).unwrap()
    );
}

#[test]
fn test_object_match() {
    assert_eq!(
        parse_to_sql("@match_object_meta($a.b.c?(($x==1 && $y==2) || ($x==$y && $z!=1)))").unwrap(),
        "FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.a.b.c' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.a.b.c ? (@!=null && ((@.x==1&&@.y==2)||(@.x==@.y&&(@.z!=1 || @.z.type()!=\"number\"))))'))"
    );
    assert_eq!(
        parse_to_sql(r#"@match_object_meta($a.b.c?($x=="A"))"#).unwrap(),
        "FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.a.b.c' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.a.b.c ? (@!=null && (@.x==\"A\"))'))"
    );
    assert_eq!(
        parse_to_sql("${x}=123; @match_object_meta($a.b.c?($x==${x} || $y>${x}))").unwrap(),
        "FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.a.b.c' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.a.b.c ? (@!=null && (@.x==123||@.y>123))'))"
    );
    assert_eq!(
        parse_to_sql(r#"@match_object_meta($a.b.c?($x regex("foo") || $y iregex("foo") || $z starts_with("foo")))"#).unwrap(),
        "FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.a.b.c' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.a.b.c ? (@!=null && (@.x like_regex \"foo\"||@.y like_regex \"foo\" flag \"i\"||@.z starts with \"foo\"))'))"
    );
    assert_eq!(
        parse_to_sql(r#"${x}="foo"; @match_object_meta($a.b.c?($x regex(${x})))"#).unwrap(),
        "FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.a.b.c' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.a.b.c ? (@!=null && (@.x like_regex \"foo\"))'))"
    );
    assert_eq!(
        parse_to_sql(r#"${x}="X"; @match_object_meta($array?($key=="From" && $value == ${x}))"#)
            .unwrap(),
        "FROM objects AS \"objects_0\" WHERE ((\"objects_0\".result @? '$.ok.object_metadata.array' AND \"objects_0\".result->'ok'->'object_metadata' @? '$.array ? (@!=null && (@.key==\"From\"&&@.value==\"X\"))'))"
    )
}
