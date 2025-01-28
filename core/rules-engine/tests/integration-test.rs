mod data;
#[cfg(test)]
pub mod test {
    use std::{
        collections::{HashMap, VecDeque},
        env, io, ptr,
        time::Duration,
    };

    use crate::data::{self, Object, Work};
    use postgres::{Client, NoTls};
    use postgresql_embedded::{blocking::PostgreSQL, Settings};

    struct Postgres {
        _server: PostgreSQL,
        client: Client,
    }

    fn initialize_postgres(data: &[Work]) -> Postgres {
        println!("Initialize postgres database...");
        let settings = Settings {
            timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        };
        let mut postgresql = PostgreSQL::new(settings);
        postgresql.setup().unwrap();
        postgresql.start().unwrap();
        let database_name = "test";
        postgresql.create_database(database_name).unwrap();
        let settings = postgresql.settings();
        println!("Create postgres tables...");
        let mut client = Client::connect(
            format!(
                "host={host} port={port} user={username} password={password} dbname=test",
                host = settings.host,
                port = settings.port,
                username = settings.username,
                password = settings.password
            )
            .as_str(),
            NoTls,
        )
        .unwrap();
        let query = include_str!("postgres.sql");
        client.batch_execute(query).unwrap();
        let statement = client.prepare(
                "INSERT INTO objects \
                (org, work_id, is_entry, object_id, object_type, object_subtype, recursion_level, size, hashes, t, result) \
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
                RETURNING id"
            ).unwrap();
        let statement2 = client
            .prepare("INSERT INTO rels (parent, child, props) VALUES ($1,$2,$3)")
            .unwrap();
        for work in data {
            let mut queue = VecDeque::new();
            queue.push_back(&work.root);
            let mut parent_map = HashMap::<*const Object, (Option<i64>, i32)>::new();
            while let Some(object) = queue.pop_front() {
                let (parent, recursion_level) = parent_map
                    .get(&(object as *const Object))
                    .map(|(a, b)| (*a, *b))
                    .unwrap_or((None, 1));

                let is_entry = ptr::eq(object, &work.root);
                let t = work.creation_time.assume_utc();
                let result = client
                    .query(
                        &statement,
                        &[
                            &work.org,
                            &work.work_id,
                            &is_entry,
                            &object.object_id,
                            &object.object_type,
                            &object.object_subtype,
                            &recursion_level,
                            &object.size,
                            &object.hashes,
                            &t,
                            &object.result,
                        ],
                    )
                    .unwrap();
                let id: i64 = result.first().unwrap().get(0);
                for child in &object.children {
                    parent_map.insert(child as *const Object, (Some(id), recursion_level + 1));
                    queue.push_back(child);
                }
                client
                    .query(&statement2, &[&parent, &id, &object.relation])
                    .unwrap();
            }
        }

        Postgres {
            _server: postgresql,
            client,
        }
    }

    struct Test {
        postgres: Postgres,
    }

    #[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
    struct ER(String, String);

    fn er(work_id: &str, object_id: &str) -> ER {
        ER(work_id.to_string(), object_id.to_string())
    }

    impl Test {
        fn new() -> Self {
            let data = data::prepare_data();
            let postgres = initialize_postgres(&data);
            Self { postgres }
        }
        fn execute_postgres(&mut self, query: &str) -> Result<Vec<ER>, io::Error> {
            let mut result = Vec::new();
            let query = pgrules::parse_to_sql(query)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            let rows = self
                .postgres
                .client
                .query(&format!("SELECT work_id, object_id {query}"), &[])
                .map_err(|e| {
                    println!("Postgres query: {query}");
                    io::Error::new(io::ErrorKind::InvalidInput, e)
                })?;

            for row in rows {
                let work_id: String = row.get(0);
                let object_id: String = row.get(1);
                result.push(ER(work_id, object_id))
            }

            Ok(result)
        }
        fn test_query(
            &mut self,
            query: &str,
            expected_results: &mut [ER],
        ) -> Result<(), io::Error> {
            expected_results.sort();
            let mut results = self.execute_postgres(query)?;
            results.sort();
            if results != expected_results {
                println!("QUERY: {query}");
                println!("--Postgres: {results:?}\n--Expected: {expected_results:?}");
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Postgres result does not match expected results",
                ));
            }
            Ok(())
        }
    }

    #[test]
    fn test_main() {
        if nix::unistd::getuid().is_root() {
            let username =
                env::var("TEST_USER").expect("Environment variable TEST_USER is not defined");
            let error_message = format!("Unable to find user '{username}'");
            let user = nix::unistd::User::from_name(&username)
                .expect(&error_message)
                .expect(&error_message);
            nix::unistd::setuid(user.uid).unwrap();
            env::set_var("HOME", user.dir);
        }
        let mut test = Test::new();
        //test.test_query("", &mut []).unwrap();
        test.test_query(
            r#"@has_object_meta($"injection'; delete from objects; --")"#,
            &mut [],
        )
        .unwrap();
        test.test_query(
            "object_type=\"Zip\"",
            &mut [
                er("work_001", "object_001_001"),
                er("work_002", "object_002_001"),
                er("work_003", "object_003_001"),
                er("work_003", "object_003_002"),
                er("work_003", "object_003_008"),
            ],
        )
        .unwrap();
        test.test_query(
            "@is_root() && object_type=\"Zip\"",
            &mut [
                er("work_001", "object_001_001"),
                er("work_002", "object_002_001"),
                er("work_003", "object_003_001"),
            ],
        )
        .unwrap();
        test.test_query(
            "@get_hash(\"md5\")==\"00cf96e7b9b95dfdd83f44ba0683523d\"",
            &mut [er("work_001", "object_001_001")],
        )
        .unwrap();
        test.test_query(
            "(object_subtype == \"PNG\" || object_subtype == \"BMP\") && size > 300000",
            &mut [
                er("work_002", "object_002_002"),
                er("work_002", "object_002_004"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_parent(object_type=\"Rar\") && @get_hash(\"md5\") !=  \"c37d895fc01b3405f44f1973a56ae68b\"",
            &mut [
                er("work_004", "object_004_003"),
                er("work_004", "object_004_004"),
                er("work_004", "object_004_005")
            ]
        )
            .unwrap();
        test.test_query(
            "@is_root() && @count_children()==0",
            &mut [er("work_005", "object_005_001")],
        )
        .unwrap();
        test.test_query(
            "@count_children(object_type=\"PE\")==2",
            &mut [er("work_004", "object_004_001")],
        )
        .unwrap();
        test.test_query(
            "@has_name(\"broken.7z\") || @has_name(regex(\"database\")) || @has_name(iregex(\"\\\\.exe$\")) || @has_name(starts_with(\"baner\"))",
            &mut [
                er("work_002", "object_002_003"),
                er("work_002", "object_002_004"),
                er("work_003", "object_003_002"),
                er("work_003", "object_003_012"),
                er("work_004", "object_004_002"),
                er("work_005", "object_005_001")
            ],
        )
        .unwrap();
        test.test_query(
            r#"@has_name("broken.7z") || @has_name(regex("database")) || @has_name(iregex(r"\.exe$")) || @has_name(starts_with("baner"))"#,
            &mut [
                er("work_002", "object_002_003"),
                er("work_002", "object_002_004"),
                er("work_003", "object_003_002"),
                er("work_003", "object_003_012"),
                er("work_004", "object_004_002"),
                er("work_005", "object_005_001")
            ],
        )
        .unwrap();
        test.test_query(
            "recursion_level>1 && @has_sibling(object_type=\"Lnk\")",
            &mut [
                er("work_001", "object_001_002"),
                er("work_001", "object_001_004"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_symbol(\"TOP_SECRET\") and org==\"my_org\"",
            &mut [er("work_001", "object_001_001")],
        )
        .unwrap();
        test.test_query(
            "@has_symbol(regex(\"SECRET\")) and org==\"my_org\"",
            &mut [er("work_001", "object_001_001")],
        )
        .unwrap();
        test.test_query(
            "@has_symbol(iregex(\"secret\")) and org==\"my_org\"",
            &mut [er("work_001", "object_001_001")],
        )
        .unwrap();
        test.test_query(
            "@has_symbol(starts_with(\"TOP_\")) and org==\"my_org\"",
            &mut [er("work_001", "object_001_001")],
        )
        .unwrap();
        test.test_query(
            "@has_descendant(object_type=\"Text\" && size==3000)",
            &mut [
                er("work_003", "object_003_001"),
                er("work_003", "object_003_008"),
            ],
        )
        .unwrap();
        test.test_query(
            "size > 800000 && @has_ancestor(object_type==\"Zip\" && size==999999)",
            &mut [
                er("work_003", "object_003_002"),
                er("work_003", "object_003_003"),
                er("work_003", "object_003_004"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_child(object_type==\"Zip\")",
            &mut [er("work_003", "object_003_001")],
        )
        .unwrap();
        test.test_query(
            "@has_root(size==3000000)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_001", "object_001_002"),
                er("work_001", "object_001_003"),
                er("work_001", "object_001_004"),
                er("work_001", "object_001_005"),
            ],
        )
        .unwrap();
        test.test_query(
            "@count_ancestors() == 2 && size < 5000",
            &mut [
                er("work_003", "object_003_009"),
                er("work_003", "object_003_010"),
                er("work_003", "object_003_011"),
                er("work_003", "object_003_012"),
                er("work_003", "object_003_013"),
            ],
        )
        .unwrap();
        test.test_query(
            "@count_ancestors(object_type!=\"Zip\") == 1",
            &mut [
                er("work_001", "object_001_005"),
                er("work_004", "object_004_002"),
                er("work_004", "object_004_003"),
                er("work_004", "object_004_004"),
                er("work_004", "object_004_005"),
            ],
        )
        .unwrap();
        test.test_query(
            "@count_siblings() > 4",
            &mut [
                er("work_002", "object_002_002"),
                er("work_002", "object_002_003"),
                er("work_002", "object_002_004"),
                er("work_002", "object_002_005"),
                er("work_002", "object_002_006"),
                er("work_002", "object_002_007"),
            ],
        )
        .unwrap();
        test.test_query(
            "@count_siblings(object_type=\"Zip\")==1",
            &mut [
                er("work_003", "object_003_002"),
                er("work_003", "object_003_008"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@is_root() && @date_range("2000-01-02","2000-01-03")"#,
            &mut [
                er("work_002", "object_002_001"),
                er("work_003", "object_003_001"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@is_root() && @date_range("2000-01-03 01:00:00","2000-01-03 01:00:00")"#,
            &mut [],
        )
        .unwrap();

        test.test_query(
            r#"@is_root() && @date_since("2000-01-04")"#,
            &mut [
                er("work_004", "object_004_001"),
                er("work_005", "object_005_001"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_object_meta($bool)",
            &mut [
                er("work_001", "object_001_004"),
                er("work_003", "object_003_008"),
                er("work_003", "object_003_010"),
            ],
        )
        .unwrap();
        test.test_query(
            "@match_object_meta($bool==true)",
            &mut [
                er("work_001", "object_001_004"),
                er("work_003", "object_003_008"),
            ],
        )
        .unwrap();
        test.test_query(
            "@match_object_meta($int1==1)",
            &mut [er("work_003", "object_003_008")],
        )
        .unwrap();
        test.test_query(
            "@match_object_meta($int2==1)",
            &mut [er("work_003", "object_003_008")],
        )
        .unwrap();
        test.test_query(
            "@match_object_meta($int1==$int2)",
            &mut [er("work_003", "object_003_008")],
        )
        .unwrap();
        test.test_query(
            "@match_object_meta($int1!=$int2)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_001", "object_001_002"),
                er("work_001", "object_001_003"),
                er("work_001", "object_001_004"),
                er("work_001", "object_001_005"),
                er("work_002", "object_002_001"),
                er("work_002", "object_002_002"),
                er("work_002", "object_002_003"),
                er("work_002", "object_002_004"),
                er("work_002", "object_002_005"),
                er("work_002", "object_002_006"),
                er("work_002", "object_002_007"),
                er("work_003", "object_003_001"),
                er("work_003", "object_003_002"),
                er("work_003", "object_003_003"),
                er("work_003", "object_003_004"),
                er("work_003", "object_003_005"),
                er("work_003", "object_003_006"),
                er("work_003", "object_003_007"),
                er("work_003", "object_003_009"),
                er("work_003", "object_003_010"),
                er("work_003", "object_003_011"),
                er("work_003", "object_003_012"),
                er("work_003", "object_003_013"),
                er("work_004", "object_004_001"),
                er("work_004", "object_004_002"),
                er("work_004", "object_004_003"),
                er("work_004", "object_004_004"),
                er("work_004", "object_004_005"),
                er("work_005", "object_005_001"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_object_meta($int1) && @match_object_meta($int1!=$int2)",
            &mut [
                er("work_001", "object_001_004"),
                er("work_003", "object_003_010"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($string == "string")"#,
            &mut [er("work_003", "object_003_008")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($string starts_with("string"))"#,
            &mut [
                er("work_001", "object_001_004"),
                er("work_003", "object_003_008"),
                er("work_003", "object_003_010"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($string regex("1"))"#,
            &mut [er("work_001", "object_001_004")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($string iregex("STRING"))"#,
            &mut [
                er("work_001", "object_001_004"),
                er("work_003", "object_003_008"),
                er("work_003", "object_003_010"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_relation_meta($nested)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_002", "object_002_002"),
                er("work_003", "object_003_003"),
                er("work_003", "object_003_008"),
                er("work_003", "object_003_010"),
                er("work_004", "object_004_005"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_relation_meta($nested.key)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_002", "object_002_002"),
                er("work_003", "object_003_003"),
                er("work_003", "object_003_008"),
                er("work_003", "object_003_010"),
            ],
        )
        .unwrap();
        test.test_query(
            "@match_relation_meta($nested.key > 0)",
            &mut [
                er("work_003", "object_003_003"),
                er("work_003", "object_003_008"),
            ],
        )
        .unwrap();
        test.test_query(
            "@match_relation_meta($nested.key == 1)",
            &mut [er("work_003", "object_003_003")],
        )
        .unwrap();
        test.test_query(
            "@match_relation_meta($nested.key != 1)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_001", "object_001_002"),
                er("work_001", "object_001_003"),
                er("work_001", "object_001_004"),
                er("work_001", "object_001_005"),
                er("work_002", "object_002_001"),
                er("work_002", "object_002_002"),
                er("work_002", "object_002_003"),
                er("work_002", "object_002_004"),
                er("work_002", "object_002_005"),
                er("work_002", "object_002_006"),
                er("work_002", "object_002_007"),
                er("work_003", "object_003_001"),
                er("work_003", "object_003_002"),
                er("work_003", "object_003_004"),
                er("work_003", "object_003_005"),
                er("work_003", "object_003_006"),
                er("work_003", "object_003_007"),
                er("work_003", "object_003_008"),
                er("work_003", "object_003_009"),
                er("work_003", "object_003_010"),
                er("work_003", "object_003_011"),
                er("work_003", "object_003_012"),
                er("work_003", "object_003_013"),
                er("work_004", "object_004_001"),
                er("work_004", "object_004_002"),
                er("work_004", "object_004_003"),
                er("work_004", "object_004_004"),
                er("work_004", "object_004_005"),
                er("work_005", "object_005_001"),
            ],
        )
        .unwrap();
        test.test_query(
            "@match_relation_meta($nested.key == $nested.key2)",
            &mut [er("work_003", "object_003_010")],
        )
        .unwrap();
        test.test_query(
            "@match_relation_meta($nested.key <> $nested.key2)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_001", "object_001_002"),
                er("work_001", "object_001_003"),
                er("work_001", "object_001_004"),
                er("work_001", "object_001_005"),
                er("work_002", "object_002_001"),
                er("work_002", "object_002_002"),
                er("work_002", "object_002_003"),
                er("work_002", "object_002_004"),
                er("work_002", "object_002_005"),
                er("work_002", "object_002_006"),
                er("work_002", "object_002_007"),
                er("work_003", "object_003_001"),
                er("work_003", "object_003_002"),
                er("work_003", "object_003_003"),
                er("work_003", "object_003_004"),
                er("work_003", "object_003_005"),
                er("work_003", "object_003_006"),
                er("work_003", "object_003_007"),
                er("work_003", "object_003_008"),
                er("work_003", "object_003_009"),
                er("work_003", "object_003_011"),
                er("work_003", "object_003_012"),
                er("work_003", "object_003_013"),
                er("work_004", "object_004_001"),
                er("work_004", "object_004_002"),
                er("work_004", "object_004_003"),
                er("work_004", "object_004_004"),
                er("work_004", "object_004_005"),
                er("work_005", "object_005_001"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@is_root() && org=="\"The Company\"""#,
            &mut [er("work_003", "object_003_001")],
        )
        .unwrap();
        test.test_query(
            r#"@is_root() && org=="żółte\U0001F332""#,
            &mut [er("work_004", "object_004_001")],
        )
        .unwrap();
        test.test_query(
            r#"@is_root() && org=="one\ttwo\nthree""#,
            &mut [er("work_005", "object_005_001")],
        )
        .unwrap();
        test.test_query(
            "@has_object_meta($test_escaping)",
            &mut [
                er("work_001", "object_001_002"),
                er("work_001", "object_001_003"),
                er("work_001", "object_001_005"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($test_escaping starts_with("\U0001F332"))"#,
            &mut [er("work_001", "object_001_003")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($test_escaping starts_with("🌲"))"#,
            &mut [er("work_001", "object_001_003")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($test_escaping starts_with(r"🌲"))"#,
            &mut [er("work_001", "object_001_003")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($test_escaping == "\u0008\u000C\u0030")"#,
            &mut [er("work_001", "object_001_005")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($test_escaping regex("\u000C"))"#,
            &mut [er("work_001", "object_001_005")],
        )
        .unwrap();
        test.test_query("@has_error()", &mut [er("work_005", "object_005_001")])
            .unwrap();
        test.test_query(
            r#"@has_error("Invalid header")"#,
            &mut [er("work_005", "object_005_001")],
        )
        .unwrap();
        test.test_query(
            r#"@has_error(starts_with("Invalid"))"#,
            &mut [er("work_005", "object_005_001")],
        )
        .unwrap();
        test.test_query(
            r#"@has_error(regex("header"))"#,
            &mut [er("work_005", "object_005_001")],
        )
        .unwrap();
        test.test_query(
            r#"@has_error(iregex("HEADER"))"#,
            &mut [er("work_005", "object_005_001")],
        )
        .unwrap();
        test.test_query(
            r#"@is_leaf()"#,
            &mut [
                er("work_001", "object_001_002"),
                er("work_001", "object_001_003"),
                er("work_001", "object_001_005"),
                er("work_002", "object_002_002"),
                er("work_002", "object_002_003"),
                er("work_002", "object_002_004"),
                er("work_002", "object_002_005"),
                er("work_002", "object_002_006"),
                er("work_002", "object_002_007"),
                er("work_003", "object_003_003"),
                er("work_003", "object_003_004"),
                er("work_003", "object_003_005"),
                er("work_003", "object_003_006"),
                er("work_003", "object_003_007"),
                er("work_003", "object_003_009"),
                er("work_003", "object_003_010"),
                er("work_003", "object_003_011"),
                er("work_003", "object_003_012"),
                er("work_003", "object_003_013"),
                er("work_004", "object_004_002"),
                er("work_004", "object_004_003"),
                er("work_004", "object_004_004"),
                er("work_004", "object_004_005"),
                er("work_005", "object_005_001"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($string.len()==6)"#,
            &mut [er("work_003", "object_003_008")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($string.len()!=6)"#,
            &mut [
                er("work_001", "object_001_004"),
                er("work_003", "object_003_010"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($test_escaping.len()>3)"#,
            &mut [er("work_001", "object_001_002")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($test_escaping.len()<5)"#,
            &mut [
                er("work_001", "object_001_003"),
                er("work_001", "object_001_005"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_object_meta($array)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_001", "object_001_004"),
                er("work_002", "object_002_004"),
                er("work_003", "object_003_008"),
                er("work_003", "object_003_010"),
                er("work_004", "object_004_004"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_object_meta($array.key)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_002", "object_002_004"),
                er("work_004", "object_004_004"),
            ],
        )
        .unwrap();
        test.test_query(
            "@has_object_meta($array.value)",
            &mut [
                er("work_001", "object_001_001"),
                er("work_002", "object_002_004"),
                er("work_004", "object_004_004"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($array?($key=="From" && $value=="A"))"#,
            &mut [er("work_001", "object_001_001")],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($array?($value regex("C")))"#,
            &mut [
                er("work_001", "object_001_001"),
                er("work_002", "object_002_004"),
                er("work_004", "object_004_004"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($array?($key=="Subject" && $value regex("C")))"#,
            &mut [
                er("work_001", "object_001_001"),
                er("work_004", "object_004_004"),
            ],
        )
        .unwrap();
        test.test_query(
            r#"@match_object_meta($array?($key=="From" && $value != 1))"#,
            &mut [
                er("work_001", "object_001_001"),
                er("work_002", "object_002_004"),
            ],
        )
        .unwrap();
        // test.test_query("", &mut []).unwrap();
    }
}
