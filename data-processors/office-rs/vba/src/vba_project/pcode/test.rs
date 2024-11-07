use super::*;

const BLTIN: (u16, &str) = (0x102, "var_bltin");
const STRSTAR: (u16, &str) = (0x103, "var_string_star");
const NOAS: (u16, &str) = (0x104, "var_noas");
const OBJ: (u16, &str) = (0x105, "var_obj");

struct DecTester<'a> {
    dec: ModuleDecompiler<'a>,
}

impl<'a> DecTester<'a> {
    fn new(project: &'a ProjectPC) -> Self {
        Self {
            dec: ModuleDecompiler {
                project,
                imptbl: ImpTbl { buf: Vec::new() },
                type_names: TypeNames {
                    sys_kind: 3,
                    header: Vec::new(),
                    types: HashMap::new(),
                    unk: 0,
                    vb_base: None,
                    total_types: 0,
                    reserved_types: 0,
                    mapped_types: 0,
                },
                functbl: FuncTbl {
                    sys_kind: 3,
                    has_phantoms: false,
                    _header: Vec::new(),
                    data: Vec::new(),
                },
                listing: Vec::new(),
                codebuf: Vec::new(),
                nlines: 0,
                noncont: 0,
                is_5x: false,
            },
        }
    }

    fn set_code(&mut self, code: &[u8]) {
        self.dec.codebuf = code.to_owned();
        self.dec.listing = vec![CodeLine {
            _decor: 0,
            _unk1: 0,
            _unk2: 0,
            indent: 0,
            len: code.len().try_into().unwrap(),
            _unk3: 0,
            offset: 0,
        }];
    }

    fn set_functbl(&mut self, functbl: &[u8]) {
        self.dec.functbl = FuncTbl {
            sys_kind: 3,
            has_phantoms: false,
            _header: Vec::new(),
            data: functbl.to_owned(),
        }
    }

    fn decompile(&self) -> String {
        self.dec.iter().next().unwrap().unwrap()
    }

    fn add_type(&mut self, k: u16, v: &str) {
        self.dec
            .type_names
            .types
            .insert(k.into(), Some(v.to_owned()));
    }

    fn set_imptbl(&mut self, tbl: &[u8]) {
        self.dec.imptbl.buf = tbl.to_owned();
    }
}

fn mkpc() -> ProjectPC {
    ProjectPC {
        name_idx: 0x102,
        sys_kind: 3,
        is_5x: false,
        lcid: 0,
        lcid_invoke: 0,
        codepage: 0,
        unused_string: None,
        docstring: None,
        help_file: None,
        help_context: 0,
        cookie: 0,
        lib_flags: 0,
        version_major: 0,
        version_minor: 0,
        references: Vec::new(),
        modules_data: Vec::new(),
        string_table: [BLTIN, STRSTAR, NOAS, OBJ]
            .into_iter()
            .map(|b| (b.0, b.1.to_owned()))
            .collect(),
        constants: Vec::new(),
    }
}

mod dim {
    use super::*;

    #[test]
    fn noas() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x00, 0x84, // flags
            (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x20, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x1b, 0x08, 0x24, 0x00, 0x00, 0x00, 0x0c, 0x00,
            0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), format!("Dim {}()", NOAS.1));

        #[rustfmt::skip]
        d.set_functbl(&[
            0x40, 0x84, // flags
            (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x0c, 0x00, 0x00, 0x00, // implicit variant
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), format!("Dim {}", NOAS.1));

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xd1, 0x00, // Sep
            0xac, 0x00, 0x20, 0x00, // Array val(32)
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x00, 0x84, // flags
            (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x01, 0x00,
            0x1b, 0x00, 0x20, 0x00, 0x00, 0x00, 0x0c, 0x00,
        ]);
        assert_eq!(d.decompile(), format!("Dim {}(32)", NOAS.1));

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xd1, 0x00, // Sep
            0xac, 0x00, 0x01, 0x00, // Array val(1)
            0xac, 0x00, 0x02, 0x00, // Array 2...
            0xac, 0x00, 0x03, 0x00, // ... To 3
            0xd1, 0x00, // Sep
            0xac, 0x00, 0x04, 0x00, // Array val(4)
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x00, 0x84, // flags
            (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x03, 0x00,
            0x1b, 0x00, 0x20, 0x00, 0x00, 0x00, 0x0c, 0x00,
        ]);
        assert_eq!(d.decompile(), format!("Dim {}(1, 2 To 3, 4)", NOAS.1));

        Ok(())
    }

    #[test]
    fn as_bltin() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        for (typeid, typename) in [
            (2, "Integer"),
            (3, "Long"),
            (4, "Single"),
            (5, "Double"),
            (6, "Currency"),
            (7, "Date"),
            (8, "String"),
            (9, "Object"),
            (11, "Boolean"),
            (12, "Variant"),
            (17, "Byte"),
            (20, "LongLong"),
            (0x94, "LongPtr"),
        ] {
            #[rustfmt::skip]
            d.set_functbl(&[
                0x60, 0x84, // flags
                (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
                0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
                0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
                typeid as u8, (typeid>>8) as u8, 0x00, 0x00, // builtin type
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
                0x00, 0x00, 0x00, 0x00, // arg flags
            ]);
            assert_eq!(d.decompile(), format!("Dim {} As {}", BLTIN.1, typename));
        }

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x20, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x1b, 0x08, 0x12, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, // extra
        ]);
        assert_eq!(d.decompile(), format!("Dim {}() As Integer", BLTIN.1));

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xd1, 0x00, // Sep
            0xac, 0x00, 0x02, 0x00, // Array val(2)
            0xac, 0x00, 0x03, 0x00, // Array 3...
            0xac, 0x00, 0x07, 0x00, // ... To 7
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x02, 0x00,
            0x1b, 0x00, 0x20, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, // extra
        ]);
        assert_eq!(d.decompile(), format!("Dim {}(2, 3 To 7) As Date", BLTIN.1));

        Ok(())
    }

    #[test]
    fn as_strstar() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xac, 0x00, 0x0a, 0x00, // String multiplier (10)
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);

        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (STRSTAR.0<<1) as u8, (STRSTAR.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x20, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x20, 0x00, // extra
        ]);
        assert_eq!(d.decompile(), format!("Dim {} As String * 10", STRSTAR.1));

        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (STRSTAR.0<<1) as u8, (STRSTAR.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x00, 0x00,
            0x1b, 0x08, 0x20, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, // extra
        ]);
        assert_eq!(d.decompile(), format!("Dim {}() As String * 10", STRSTAR.1));

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xd1, 0x00, // Sep
            0xac, 0x00, 0x01, 0x00, // Array val(1)
            0xac, 0x00, 0x1e, 0x00, // String multiplier (30)
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00,  // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (STRSTAR.0<<1) as u8, (STRSTAR.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x01, 0x00,
            0x1b, 0x00, 0x20, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, // extra
        ]);
        assert_eq!(
            d.decompile(),
            format!("Dim {}(1) As String * 30", STRSTAR.1)
        );

        Ok(())
    }

    #[test]
    fn as_obj() -> Result<(), io::Error> {
        const COLLECTION: u16 = 0x5;
        const DOCUMENT: u16 = 0x58;
        let pc = mkpc();
        let mut d = DecTester::new(&pc);
        d.add_type(COLLECTION, "Collection");
        d.add_type(DOCUMENT, "Word.Document");

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x20, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x1d, 0x00, (COLLECTION<<3) as u8, ((COLLECTION<<3)>>8) as u8, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), format!("Dim {} As Collection", OBJ.1));

        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x00, 0x00,
            0x1b, 0x08, 0x20, 0x00, 0x00, 0x00, 0x1d, 0x00, (DOCUMENT<<3) as u8, ((DOCUMENT<<3)>>8) as u8,
        ]);
        assert_eq!(d.decompile(), format!("Dim {}() As Word.Document", OBJ.1));

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xac, 0x00, 0x01, 0x00, // Array 2...
            0xac, 0x00, 0x03, 0x00, // ... To 3
            0xd1, 0x00, // Sep
            0xac, 0x00, 0x05, 0x00, // Array val(5)
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x02, 0x00, // array items
            0x1b, 0x00, 0x20, 0x00, 0x00, 0x00, 0x1d, 0x00, (COLLECTION<<3) as u8, ((COLLECTION<<3)>>8) as u8
        ]);
        assert_eq!(
            d.decompile(),
            format!("Dim {}(1 To 3, 5) As Collection", OBJ.1)
        );
        Ok(())
    }

    #[test]
    fn as_new_obj() -> Result<(), io::Error> {
        const COLLECTION: u16 = 0x5;
        const DOCUMENT: u16 = 0x58;
        let pc = mkpc();
        let mut d = DecTester::new(&pc);
        d.add_type(COLLECTION, "Collection");
        d.add_type(DOCUMENT, "Word.Document");

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0xa4, // flags
            (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x20, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x1d, 0x00, (COLLECTION<<3) as u8, ((COLLECTION<<3)>>8) as u8, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), format!("Dim {} As New Collection", OBJ.1));

        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0xa4, // flags
            (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x00, 0x00,
            0x1b, 0x08, 0x20, 0x00, 0x00, 0x00, 0x1d, 0x00, (DOCUMENT<<3) as u8, ((DOCUMENT<<3)>>8) as u8,
        ]);
        assert_eq!(
            d.decompile(),
            format!("Dim {}() As New Word.Document", OBJ.1)
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim
            0xac, 0x00, 0x01, 0x00, // Array 2...
            0xac, 0x00, 0x03, 0x00, // ... To 3
            0xd1, 0x00, // Sep
            0xac, 0x00, 0x05, 0x00, // Array val(5)
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0xa4, // flags
            (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x22, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x02, 0x00, // array items
            0x1b, 0x00, 0x20, 0x00, 0x00, 0x00, 0x1d, 0x00, (COLLECTION<<3) as u8, ((COLLECTION<<3)>>8) as u8
        ]);
        assert_eq!(
            d.decompile(),
            format!("Dim {}(1 To 3, 5) As New Collection", OBJ.1)
        );
        Ok(())
    }

    #[test]
    fn multiple_vars() -> Result<(), io::Error> {
        const DOCUMENT: u16 = 0x58;
        let pc = mkpc();
        let mut d = DecTester::new(&pc);
        d.add_type(DOCUMENT, "Word.Document");

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x00, // Dim

            0xd1, 0x00, // Sep
            0xac, 0x00, 0x2a, 0x00, // Array val(42)
            0xf5, 0x04, 0x26, 0x00, 0x00, 0x00, // Var offset into fn table

            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);

        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0xa4, // flags
            (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x20, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x1d, 0x00, (DOCUMENT<<3) as u8, ((DOCUMENT<<3)>>8) as u8, 0x00, 0x00,

            0x00, 0x84, // flags
            (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x48, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x01, 0x00,
            0x1b, 0x00, 0x46, 0x00, 0x00, 0x00, 0x0c, 0x00,
        ]);
        assert_eq!(
            d.decompile(),
            format!("Dim {}(42), {} As New Word.Document", NOAS.1, OBJ.1)
        );
        Ok(())
    }

    #[test]
    fn with_events() -> Result<(), io::Error> {
        const COLLECTION: u16 = 0x5;
        let pc = mkpc();
        let mut d = DecTester::new(&pc);
        d.add_type(COLLECTION, "Collection");

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x40, // Private
            0xf5, 0xc4, 0x00, 0x00, 0x00, 0x00, 0xac, 0xab, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x84, // flags
            (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x20, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x1d, 0x00, (COLLECTION<<3) as u8, ((COLLECTION<<3)>>8) as u8, 0x00, 0x00,
        ]);
        assert_eq!(
            d.decompile(),
            format!("Private WithEvents {} As Collection", OBJ.1)
        );
        Ok(())
    }

    #[test]
    fn consts() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x44, // Private Const
            0xac, 0x00, 0x0c, 0x00, // const value
            0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x42, 0x98, // flags
            (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
            0xac, 0xab,  0x00, 0x00,  0x0c, 0x00, // unk1-3
            0x00, 0x00,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x42, 0x00, 0x00, 0x00, // implicit type
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), format!("Private Const {} = 12", BLTIN.1));

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x04, // Const
            0xac, 0x00, 0x22, 0x00, // const value
            0xf5, 0x08, 0x00, 0x00, 0x00, 0x00,  // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x42, 0x98, // flags
            (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
            0xac, 0xab,  0x00, 0x00,  0x22, 0x00, // unk1-3
            0x00, 0x00,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x42, 0x00, 0x00, 0x00, // implicit type
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), format!("Const {} = 34", BLTIN.1));

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x24, // Public Const
            0xb9, 0x00, 0x03, 0x00, 0x61, 0x61, 0x61, 0x00,  // const value
            0xf5, 0x08, 0x00, 0x00, 0x00, 0x00, // Const var
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x62, 0x90, // flags
            (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
            0xac, 0xab,  0x00, 0x00,  0x38, 0x00, // unk1-3
            0x00, 0x00,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x48, 0x00, 0x00, 0x00, // implicit type
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(
            d.decompile(),
            format!("Public Const {} As String = \"aaa\"", BLTIN.1)
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x04, // Const
            0xac, 0x00, 0x04, 0x00, // * value
            0xb9, 0x00, 0x04, 0x00, 0x31, 0x32, 0x33, 0x34, // const value
            0xf5, 0x08, 0x00, 0x00, 0x00, 0x00,  // Const var
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x20, 0x94, // flags
            (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0x00, 0x00,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0xb8, 0x00, 0x00, 0x00, // implicit type
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(
            d.decompile(),
            format!("Const {} As String * 4 = \"1234\"", BLTIN.1)
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x44, // Const
            0xac, 0x00, 0x0f, 0x00, // Const value
            0xf5, 0x08, 0x00, 0x00, 0x00, 0x00, // Const var
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            0x60, 0x94, // flags
            (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
            0xff, 0xff,  0xff, 0xff,  0xff, 0xff, // unk1-3
            0x00, 0x00,  0xff, 0xff,  0xff, 0xff, // unk 4-6
            0x51, 0x00, 0x00, 0x00, // explicit type
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // nextvar etc
            0x00, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(
            d.decompile(),
            format!("Private Const {} As Byte = 15", BLTIN.1)
        );
        Ok(())
    }

    mod redim {
        use super::*;

        #[test]
        fn noas() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x01, 0x00, // 1
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x02, 0x00, // 2
                0xe4, 0x00, // redim
                (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
                0x02, 0x00, // arcnt
                0x00, 0x00, 0x00, 0x00 // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0x08, 0x00, 0x00, 0x00, // offset to cnt
                0x0c, 0x00, // bltin type
                0x02, 0x00, // ignored count
            ]);
            assert_eq!(d.decompile(), format!("ReDim {}(1, 2)", NOAS.1));
            Ok(())
        }

        #[test]
        fn preserve() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x01, 0x00, // 1
                0xac, 0x00, 0x03, 0x00, // 3
                0xac, 0x00, 0x07, 0x00, // 7
                0xe4, 0x40, // redim preserve
                (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
                0x02, 0x00, // arcnt
                0x00, 0x00, 0x00, 0x00 // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0x08, 0x00, 0x00, 0x00, // offset to cnt
                0x0c, 0x00, // bltin type
                0x02, 0x00, // ignored count
            ]);
            assert_eq!(
                d.decompile(),
                format!("ReDim Preserve {}(1, 3 To 7)", NOAS.1)
            );
            Ok(())
        }

        #[test]
        fn multi() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xac, 0x00, 0x02, 0x00, // 2
                0xac, 0x00, 0x03, 0x00, // 3
                0xe5, 0x00, // redim as
                (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
                0x01, 0x00, // arcnt
                0x00, 0x00, 0x00, 0x00, // offset
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x02, 0x00, // 2
                0xe4, 0x00, // redim
                (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
                0x01, 0x00, // arcnt
                0x08, 0x00, 0x00, 0x00 // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0xff, 0xff, 0xff, 0xff, // offset to count - unused
                0x02, 0x00,

                0x1b, 0x08, // flags
                0xff, 0xff, 0xff, 0xff, // offset to count - unused
                0x0c, 0x00,
            ]);
            assert_eq!(
                d.decompile(),
                format!("ReDim {}(2 To 3) As Integer, {}(2)", BLTIN.1, NOAS.1)
            );
            Ok(())
        }

        #[test]
        fn as_obj() -> Result<(), io::Error> {
            const COLLECTION: u16 = 0x5;
            const DOCUMENT: u16 = 0x58;
            let pc = mkpc();
            let mut d = DecTester::new(&pc);
            d.add_type(COLLECTION, "Collection");
            d.add_type(DOCUMENT, "Word.Document");

            #[rustfmt::skip]
            d.set_code(&[
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x01, 0x00, // 1
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x02, 0x00, // 2
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x03, 0x00, // 3
                0xe5, 0x00, // redim as
                (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
                0x03, 0x00, // arcnt
                0x00, 0x00, 0x00, 0x00 // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0xff, 0xff, 0xff, 0xff, // offset to count - unused
                0x1d, 0x00, // token obj
                (COLLECTION<<3) as u8, ((COLLECTION<<3)>>8) as u8,
            ]);
            assert_eq!(
                d.decompile(),
                format!("ReDim {}(1, 2, 3) As Collection", OBJ.1)
            );
            Ok(())
        }

        #[test]
        fn as_new_obj() -> Result<(), io::Error> {
            const COLLECTION: u16 = 0x5;
            const DOCUMENT: u16 = 0x58;
            let pc = mkpc();
            let mut d = DecTester::new(&pc);
            d.add_type(COLLECTION, "Collection");
            d.add_type(DOCUMENT, "Word.Document");

            #[rustfmt::skip]
            d.set_code(&[
                0x03, 0x01, // New flag
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x03, 0x00, // 3
                0xe5, 0x00, // redim as
                (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
                0x01, 0x00, // arcnt
                0x00, 0x00, 0x00, 0x00 // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0xff, 0xff, 0xff, 0xff, // offset to count - unused
                0x1d, 0x00, // token obj
                (DOCUMENT<<3) as u8, ((DOCUMENT<<3)>>8) as u8,
            ]);
            assert_eq!(
                d.decompile(),
                format!("ReDim {}(3) As New Word.Document", OBJ.1)
            );
            Ok(())
        }

        #[test]
        fn as_strstar() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x0a, 0x00, // 10
                0xac, 0x00, 0x2c, 0x01, // 300
                0xe5, 0x00, // redim as
                (BLTIN.0<<1) as u8, (BLTIN.0>>7) as u8, // var<<1
                0x01, 0x00, // arcnt
                0x00, 0x00, 0x00, 0x00, // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0xff, 0xff, 0xff, 0xff, // offset to count - unused
                0x20, 0x00,
            ]);
            assert_eq!(
                d.decompile(),
                format!("ReDim {}(10) As String * 300", BLTIN.1)
            );
            Ok(())
        }

        #[test]
        fn prop() -> Result<(), io::Error> {
            const DOCUMENT: u16 = 0x58;
            let mut pc = mkpc();
            pc.string_table.insert(0x139, "MyProperty".to_string());
            let mut d = DecTester::new(&pc);
            d.add_type(DOCUMENT, "Word.Document");

            #[rustfmt::skip]
            d.set_code(&[
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x0a, 0x00, // 10
                0x20, 0x00, (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
                0xc2, 0x40, // redim prop
                0x72, 0x02, // myprop
                0x01, 0x00, // args
                0x00, 0x00, 0x00, 0x00, // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0xff, 0xff, 0xff, 0xff, // offset to count - unused
                0x0c, 0x00, // type - unused
            ]);
            assert_eq!(
                d.decompile(),
                "ReDim Preserve var_obj.MyProperty(10)".to_string()
            );
            #[rustfmt::skip]
            d.set_code(&[
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x0a, 0x00, // 10
                0xc3, 0x00, // redim prop
                0x72, 0x02, // myprop
                0x01, 0x00, // args
                0x00, 0x00, 0x00, 0x00, // offset
            ]);
            assert_eq!(d.decompile(), "ReDim .MyProperty(10)".to_string());

            #[rustfmt::skip]
            d.set_code(&[
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x0a, 0x00, // 10
                0x20, 0x00, (OBJ.0<<1) as u8, (OBJ.0>>7) as u8, // var<<1
                0xc4, 0x00, // redim prop
                0x72, 0x02, // myprop
                0x01, 0x00, // args
                0x00, 0x00, 0x00, 0x00, // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0xff, 0xff, 0xff, 0xff, // offset to count - unused
                0x02, 0x00, // type
            ]);
            assert_eq!(
                d.decompile(),
                "ReDim var_obj.MyProperty(10) As Integer".to_string()
            );

            #[rustfmt::skip]
            d.set_code(&[
                0x03, 0x01, // as new
                0xd1, 0x00, // Sep
                0xac, 0x00, 0x0a, 0x00, // 10
                0xc5, 0x00, // redim prop
                0x72, 0x02, // myprop
                0x01, 0x00, // args
                0x00, 0x00, 0x00, 0x00, // offset
            ]);
            #[rustfmt::skip]
            d.set_functbl(&[
                0x1b, 0x08, // flags
                0xff, 0xff, 0xff, 0xff, // offset to count - unused
                0x1d, 0x00, // type
                (DOCUMENT<<3) as u8, ((DOCUMENT<<3)>>8) as u8,
            ]);
            assert_eq!(
                d.decompile(),
                "ReDim .MyProperty(10) As New Word.Document".to_string()
            );
            Ok(())
        }
    }

    #[test]
    fn implements() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x011a, "Application".to_string());
        let mut d = DecTester::new(&pc);
        d.add_type(5, "Word.Application");

        #[rustfmt::skip]
        d.set_code(&[
            0x9f, 0x04, // Implements
            0x08, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // type data
            0x1d, 0x00, 0x28, 0x00, 0x25, 0x00, 0x00, 0x00,
            // var def
            0x2a, 0x80, // flags
            0x35, 0x02, // Application (this appears to be unused)
            0xff, 0xff, 0xff, 0xff, // unk
            0xff, 0xff, 0xff, 0xff, // unk
            0xff, 0xff, 0xff, 0xff, // unk
            0x00, 0x00, 0x00, 0x00, // offset
            0x11, 0x00, 0xff, 0xff, // unk
            0xff, 0xff, 0xff, 0xff, // nextvar etc
            0xff, 0xff, 0xff, 0xff, // arg flags
        ]);
        assert_eq!(d.decompile(), "Implements Word.Application".to_string());
        Ok(())
    }
}

mod immed {
    use super::*;

    mod int {
        use super::*;

        #[test]
        fn decimal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);
            #[rustfmt::skip]
            d.set_code(&[
                0xac, 0x00, 0x0a, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 10", NOAS.1));

            #[rustfmt::skip]
            d.set_code(&[
                0xac, 0x00, 0x14, 0x00,
                0x16, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = -20", NOAS.1));

            #[rustfmt::skip]
            d.set_code(&[
                0xac, 0x00, 0xff, 0x7f,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 32767", NOAS.1));
            Ok(())
        }

        #[test]
        fn octal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xb3, 0x00, 0x07, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = &O7", NOAS.1));
            Ok(())
        }

        #[test]
        fn hexadecimal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xaf, 0x00, 0xab, 0xac,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = &HACAB", NOAS.1));
            Ok(())
        }
    }

    mod long {
        use super::*;

        #[test]
        fn decimal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xad, 0x00, 0x0a, 0x00, 0x00, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 10&", NOAS.1));

            #[rustfmt::skip]
            d.set_code(&[
                0xad, 0x00, 0x40, 0xe2, 0x01, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 123456", NOAS.1));
            Ok(())
        }

        #[test]
        fn octal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xb4, 0x00, 0x77, 0x39, 0x05, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = &O1234567", NOAS.1));
            Ok(())
        }

        #[test]
        fn hexadecimal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xb0, 0x00, 0x37, 0x13, 0xab, 0xac,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = &HACAB1337", NOAS.1));

            #[rustfmt::skip]
            d.set_code(&[
                0xb0, 0x00, 0x00, 0xff, 0x00, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = &HFF00&", NOAS.1));
            Ok(())
        }
    }

    mod longlong {
        use super::*;

        #[test]
        fn decimal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xae, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 10^", NOAS.1));

            #[rustfmt::skip]
            d.set_code(&[
                0xae, 0x00, 0x4e, 0xdc, 0xd5, 0xdb, 0x4b, 0x9b, 0xb6, 0x01,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 123456789912345678^", NOAS.1));
            Ok(())
        }

        #[test]
        fn octal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xb5, 0x00, 0x88, 0xc6, 0xfa, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = &O76543210^", NOAS.1));
            Ok(())
        }

        #[test]
        fn hexadecimal() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xb1, 0x00, 0xab, 0xac, 0x73, 0x33, 0x31, 0x00, 0x00, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = &H313373ACAB^", NOAS.1));
            Ok(())
        }
    }

    #[test]
    fn special() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xba, 0x00, // False
            0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
        ]);
        assert_eq!(d.decompile(), format!("{} = False", NOAS.1));

        #[rustfmt::skip]
        d.set_code(&[
            0xba, 0x04, // True
            0x27, 0x00, (NOAS.0 << 1) as u8, (NOAS.0 >> 7) as u8,
        ]);
        assert_eq!(d.decompile(), format!("{} = True", NOAS.1));

        #[rustfmt::skip]
        d.set_code(&[
            0xba, 0x08, // Null
            0x27, 0x00, (NOAS.0 << 1) as u8, (NOAS.0 >> 7) as u8,
        ]);
        assert_eq!(d.decompile(), format!("{} = Null", NOAS.1));

        #[rustfmt::skip]
        d.set_code(&[
            0xba, 0x0c, // Empty
            0x27, 0x00, (NOAS.0 << 1) as u8, (NOAS.0 >> 7) as u8,
        ]);
        assert_eq!(d.decompile(), format!("{} = Empty", NOAS.1));

        Ok(())
    }

    mod decimals {
        use super::*;

        #[test]
        fn single() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xb6, 0x00, 0xf7, 0xbf, 0xad, 0x2b,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 1.2345670E-12!", NOAS.1));

            #[rustfmt::skip]
            d.set_code(&[
                0xb6, 0x00, 0xaa, 0x54, 0xab, 0x60,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 9.8765435E19!", NOAS.1));

            Ok(())
        }

        #[test]
        fn double() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xb7, 0x00, 0x66, 0xde, 0x77, 0x83, 0x21, 0x12, 0xdc, 0x42,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 1.234567890123456E14", NOAS.1));
            Ok(())
        }

        #[test]
        fn currency() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0xa9, 0x00, 0x39, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 1.2345@", NOAS.1));

            #[rustfmt::skip]
            d.set_code(&[
                0xa9, 0x00, 0x90, 0x66, 0xe9, 0x7d, 0xf4, 0x10, 0x22, 0x11,
                0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
            ]);
            assert_eq!(d.decompile(), format!("{} = 123456789012345@", NOAS.1));
            Ok(())
        }
    }

    #[test]
    fn date() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xaa, 0x00, 0x39, 0x99, 0xa4, 0x5a, 0xbb, 0xc6, 0xe0, 0x3f,
            0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
        ]);
        assert_eq!(d.decompile(), format!("{} = #12:34:56 PM#", NOAS.1));

        #[rustfmt::skip]
        d.set_code(&[
            0xaa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x39, 0xe5, 0x40,
            0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
        ]);
        assert_eq!(d.decompile(), format!("{} = #2019-01-01#", NOAS.1));

        #[rustfmt::skip]
        d.set_code(&[
            0xaa, 0x00, 0xaa, 0xb5, 0x6b, 0x0c, 0x01, 0x58, 0x21, 0x41,
            0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
        ]);
        assert_eq!(
            d.decompile(),
            format!("{} = #3456-01-02 12:34:56 PM#", NOAS.1)
        );

        Ok(())
    }

    #[test]
    fn string() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x1a, 0x00, 0x54, 0x68, 0x69, 0x73,
            0x20, 0x69, 0x73, 0x20, 0x61, 0x20, 0x22, 0x53,
            0x74, 0x72, 0x69, 0x6e, 0x67, 0x22, 0x20, 0x6c,
            0x69, 0x74, 0x65, 0x72, 0x61, 0x6c,
            0x27, 0x00, (NOAS.0<<1) as u8, (NOAS.0>>7) as u8,
        ]);
        assert_eq!(
            d.decompile(),
            format!("{} = \"This is a \"\"String\"\" literal\"", NOAS.1)
        );
        Ok(())
    }
}

mod pound {
    use super::*;

    #[test]
    fn consts() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x205, "MyConst".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x00, 0x01, // #
            0xac, 0x00, 0x01, 0x00, // 1
            0xfb, 0x00, 0x0a, 0x04, // 0x205<<1
        ]);
        assert_eq!(d.decompile(), "#Const MyConst = 1");
        Ok(())
    }

    #[test]
    fn ifdefs() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x10a, "Win32".to_string());
        pc.string_table.insert(0x10c, "Mac".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x00, 0x01, // #
            0x20, 0x00, 0x18, 0x02, // Mac
            0xfc, 0x00, // If
        ]);
        assert_eq!(d.decompile(), "#If Mac Then");

        #[rustfmt::skip]
        d.set_code(&[
            0x00, 0x01, // #
            0x20, 0x00, 0x14, 0x02, // Win32
            0xfe, 0x00, // ElseIf
        ]);
        assert_eq!(d.decompile(), "#ElseIf Win32 Then");

        #[rustfmt::skip]
        d.set_code(&[
            0x00, 0x01, // #
            0xfd, 0x00, // Else
        ]);
        assert_eq!(d.decompile(), "#Else");

        #[rustfmt::skip]
        d.set_code(&[
            0x00, 0x01, // #
            0xff, 0x00, // End If
        ]);
        assert_eq!(d.decompile(), "#End If");

        Ok(())
    }
}

mod procedure {
    use super::*;

    #[test]
    fn sub() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x116, "MySub".to_string());
        pc.string_table.insert(0x117, "a".to_string());
        pc.string_table.insert(0xc, "b".to_string());
        pc.string_table.insert(0x11b, "c".to_string());
        pc.string_table.insert(0x11c, "d".to_string());
        pc.string_table.insert(0x11d, "e".to_string());
        let mut d = DecTester::new(&pc);
        d.add_type(3, "Word.Document");

        #[rustfmt::skip]
        d.set_code(&[
            0xfa, 0x00, // have stuff on the stack
            0xac, 0x00, 0x14, 0x00, // 20
            0x96, 0x04, 0x00, 0x00, 0x00, 0x00, // sub definition
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0c, 0x11, // sub flags
            0x2c, 0x02, // sub nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0x00, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0xff, 0xff, 0xff, 0xff,
            0xe0, 0x00, 0xff, 0xff,
            0x04, 0x00, 0x04, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94,
            0x05, 0x00, // var count
            0x03,
            0x00, 0x00, 0x00, 0x00,

            // Var a
            0x49, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x0c, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0x78, 0x00, 0x00, 0x00, // next offset
            0x00, 0x00, 0x00, 0x00, // arg flags

            // Var b
            0x09, 0x83, // flags
            0x18, 0x00, // id
            0xff, 0xff, 0xff, 0xff,
            0x10, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x98, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00,
            0xa8, 0x00, 0x00, 0x00, // next offset
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x1b, 0x09, // extra type
            0xa0, 0x00, 0x00, 0x00, // array cnt offset
            0x00, 0x00, // pad
            0x00, 0x00, // array cnt
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // pad

            // Var c
            0x69, 0x83, // flags
            0x36, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x18, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x0b, 0x01, 0x00, 0x00, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xc8, 0x00, 0x00, 0x00, // next offset
            0x00, 0x00, 0x00, 0x00, // arg flags

            // Var d
            0x29, 0x83, // flags
            0x38, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x20, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xec, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00,
            0xf8, 0x00, 0x00, 0x00, // next offset
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x00, 0x00, // array cnt
            0x00, 0x00, // pad
            0x1b, 0x09, // extra type
            0xe8, 0x00, 0x00, 0x00, // offset
            0x20, 0x00,
            0x00, 0x00, 0x00, 0x00, // pad

            // Var e
            0x29, 0x83, // flags
            0x3b, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x28, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x18, 0x01, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x00, 0x00, 0x00, 0x00, // arg flags
            0x1d, 0x01, // extra type
            0x18, 0x00, 0x00, 0x00,

        ]);
        assert_eq!(
            d.decompile(),
            "Sub MySub(a, b(), c As Boolean, d() As String * 20, e As Word.Document)"
        );
        Ok(())
    }

    #[test]
    fn arg_mods() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x116, "MySub".to_string());
        pc.string_table.insert(0x117, "a".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x04, 0x00, 0x00, 0x00, 0x00, // sub definition
        ]);
        #[rustfmt::skip]
        let base_functbl: &[u8] = &[
            // Sub definition
            0x0c, 0x11, // sub flags
            0x2c, 0x02, // sub nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0x00, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94,
            0x01, 0x00, // var count
            0x02,
            0x00, 0x00, 0x00, 0x00,
        ];

        #[rustfmt::skip]
        let var: &[u8] = &[
            // Var a
            0x49, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x0c, 0x00, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x84, 0x00, 0x00, 0x00, // arg flags
        ];
        d.set_functbl(&[base_functbl, var].concat());
        assert_eq!(d.decompile(), "Sub MySub(ByVal a)");

        #[rustfmt::skip]
        let var: &[u8] = &[
            0x29, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x78, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x82, 0x01, 0x00, 0x00, // arg flags
            0x1b, 0x09, // extra type
            0x80, 0x00, 0x00, 0x00, // array cnt offset
            0x02, 0x00, // 03
            0x00, 0x00, // array cnt
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // pad
        ];
        d.set_functbl(&[base_functbl, var].concat());
        assert_eq!(d.decompile(), "Sub MySub(ByRef a() As Integer)");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0c, 0x11, // sub flags
            0x2c, 0x02, // sub nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0x00, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94,
            0x01, 0x3f, // var count
            0x02,
            0x00, 0x00, 0x00, 0x00,

            // Var a
            0x09, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x78, 0x00, 0x00, 0x00, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x80, 0x01, 0x00, 0x00, // arg flags
            0x1b, 0x09, // extra type
            0x80, 0x00, 0x00, 0x00, // array cnt offset
            0x02, 0x00, // 03
            0x00, 0x00, // array cnt
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  // pad
        ]);
        assert_eq!(d.decompile(), "Sub MySub(ParamArray a())");

        #[rustfmt::skip]
        let var: &[u8] = &[
            0x69, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x01, 0xff, 0xff, // builtin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x80, 0x03, 0x00, 0x00, // arg flags
        ];
        d.set_functbl(&[base_functbl, var].concat());
        assert_eq!(d.decompile(), "Sub MySub(Optional a As String)");

        #[rustfmt::skip]
        d.set_code(&[
            0xfa, 0x00, // have stuff on the stack
            0xac, 0x00, 0x2a, 0x00, // 42
            0x96, 0x04, 0x00, 0x00, 0x00, 0x00, // sub definition
        ]);
        #[rustfmt::skip]
        let var: &[u8] = &[
            0x49, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x0c, 0x01, 0xff, 0xff, // builtin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x80, 0x07, 0x00, 0x00, // arg flags
        ];
        d.set_functbl(&[base_functbl, var].concat());
        assert_eq!(d.decompile(), "Sub MySub(Optional a = 42)");
        Ok(())
    }

    #[test]
    fn function() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x116, "MyFunc".to_string());
        pc.string_table.insert(0x117, "a".to_string());
        let mut d = DecTester::new(&pc);
        d.add_type(4, "Word.Document");

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x08, 0x00, 0x00, 0x00, 0x00, // func definition
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Function definition
            0x0c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0x0c, 0x00, 0xff, 0xff, // ret_bltin_or_offset
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xbc, // ret type
            0x02, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var a
            0x69, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0x78, 0x00, 0x00, 0x00, // next offset
            0x80, 0x01, 0x00, 0x00, // arg flags

            // Var Me
            0x69, 0x83, // flags
            0xfe, 0xff, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x0c, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x20, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), "Function MyFunc(a As Integer)");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Function definition
            0x2c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0x02, 0x00, 0xff, 0xff, // ret_bltin_or_offset
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xbc, // ret type
            0x01, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var Me
            0x69, 0x83, // flags
            0xfe, 0xff, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x20, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), "Function MyFunc() As Integer");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Function definition
            0x2c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x60, 0x00, 0x00, 0x00, // first var offset
            0x58, 0x00, 0x00, 0x00, // ret_bltin_or_offset
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xb4, // ret type
            0x01, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // return type
            0x1b, 0x08, // extra type
            0x88, 0x00, 0x00, 0x00, // array cnt offset
            0x03, 0x00, // extra3

            // Var Me
            0x29, 0x83, // flags
            0xfe, 0xff, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x80, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x20, 0x00, 0x00, 0x00, // arg flags
            0x1b, 0x09, // extra type
            0x88, 0x00, 0x00, 0x00, // array cnt offset
            0x03, 0x00,

            0x00, 0x00, // common arg count
        ]);
        assert_eq!(d.decompile(), "Function MyFunc() As Long()");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Function definition
            0x2c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x60, 0x00, 0x00, 0x00, // first var offset
            0x58, 0x00, 0x00, 0x00, // ret_bltin_or_offset
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xb4, // ret type
            0x01, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // return type
            0x1d, 0x00, // extra type
            0x20, 0x00, // token
            0x00, 0x00, 0x00, 0x00, // pad

            // Var Me
            0x29, 0x83, // flags
            0xfe, 0xff, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x80, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x20, 0x00, 0x00, 0x00, // arg flags
            0x1d, 0x01, // extra type
            0x20, 0x00, // token
        ]);
        assert_eq!(d.decompile(), "Function MyFunc() As Word.Document");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Function definition
            0x2c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0x88, 0x00, 0x00, 0x00, // ret_bltin_or_offset
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xb4, // ret type
            0x01, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var Me
            0x29, 0x83, // flags
            0xfe, 0xff, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x78, 0x00, 0x00, 0x00, // offset
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x20, 0x00, 0x00, 0x00, // arg flags
            0x1b, 0x09, // extra type
            0x82, 0x00, 0x00, 0x00, // array cnt offset
            0x1d, 0x00, // extra3
            0x20, 0x00, // extra4 (token)
            0x00, 0x00, // array count (me)
            0x00, 0x00, // pad
            0x00, 0x00, // array count (ret)

            // return type
            0x1b, 0x08, // extra type
            0x86, 0x00, 0x00, 0x00, // array cnt offset
            0x1d, 0x00, // extra3
            0x20, 0x00, // extra4 (token)
        ]);
        assert_eq!(d.decompile(), "Function MyFunc() As Word.Document()");
        Ok(())
    }

    #[test]
    fn property() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x123, "MyProperty".to_string());
        pc.string_table.insert(0x124, "vNewValue".to_string());
        pc.string_table.insert(0x135, "P".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x08, 0x00, 0x00, 0x00, 0x00, // property get
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Property definition
            0x2c, 0x21, // func flags
            0x46, 0x02, // func nameid
            0x90, 0x05, 0x00, 0x00, // next sub
            0x07, 0x00, // ord
            // unks
            0x08, 0x64,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0x0c, 0x00, 0xff, 0xff, // ret_bltin_or_offset
            0x40, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xbc, // ret type
            0x01, // var count
            0x00, // vararg
            0x03, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var Me
            0x69, 0x83, // flags
            0xfe, 0xff, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x0c, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x20, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), "Property Get MyProperty() As Variant");

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x04, 0x00, 0x00, 0x00, 0x00, // property get
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Property definition
            0x0c, 0x41, // sub flags
            0x46, 0x02, // sub nameid
            0x90, 0x05, 0x00, 0x00, // next sub
            0x07, 0x00, // ord
            // unks
            0x08, 0x64,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x48, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x01, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var
            0x69, 0x83, // flags
            0x48, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x0c, 0x00, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x84, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(
            d.decompile(),
            "Property Let MyProperty(ByVal vNewValue As Variant)"
        );

        #[rustfmt::skip]
        d.set_functbl(&[
            // Property definition
            0x0c, 0x81, // sub flags
            0x46, 0x02, // sub nameid
            0x08, 0x06, 0x00, 0x00, // next sub
            0x07, 0x00, // ord
            // unks
            0x08, 0x64,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x50, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x01, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var
            0x69, 0x83, // flags
            0x6a, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x09, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x80, 0x01, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), "Property Set MyProperty(P As Object)");
        Ok(())
    }

    #[test]
    fn event() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x11b, "MyEvent".to_string());
        pc.string_table.insert(0x11c, "EventParam".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x74, 0x40, 0x00, 0x00, 0x00, 0x00, // event
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Event definition
            0x0c, 0x13, // func flags
            0x36, 0x02, // func nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0x01, 0x00, // ord
            // unks
            0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0xff, 0xff, 0xff, 0xff,
            0x01, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x01, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var
            0x69, 0x83, // flags
            0x38, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x80, 0x01, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), "Event MyEvent(EventParam As String)");

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x05, 0x00, 0x76, 0x61, 0x6c, 0x75, 0x65, 0x00, // "value"
            0x75, 0x00, 0x36, 0x02, 0x01, 0x00,
        ]);
        assert_eq!(d.decompile(), "RaiseEvent MyEvent(\"value\")");

        #[rustfmt::skip]
        d.set_code(&[
            0x75, 0x00, 0x36, 0x02, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "RaiseEvent MyEvent");
        Ok(())
    }

    #[test]
    fn visibility() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x116, "MySub".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x04, 0x00, 0x00, 0x00, 0x00, // sub
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0x90, 0x00, 0x00, 0x00, // next sub
            0x08, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x00, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x00, // var count
            0x00, // vararg
            0x03, // extra vis
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Sub MySub()");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xc0, 0x0c, 0x00, 0x00, // next sub
            0x0a, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x00, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x00, // var count
            0x00, // vararg
            0x01, // extra vis
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Private Sub MySub()");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xc0, 0x0c, 0x00, 0x00, // next sub
            0x0a, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x00, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x00, // var count
            0x00, // vararg
            0x07, // extra vis
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Friend Sub MySub()");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x8c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xc0, 0x0c, 0x00, 0x00, // next sub
            0x0a, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x00, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x00, // var count
            0x00, // vararg
            0x03, // extra vis
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Static Sub MySub()");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x8c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xc0, 0x0c, 0x00, 0x00, // next sub
            0x0a, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x00, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x00, // var count
            0x00, // vararg
            0x01, // extra vis
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Private Static Sub MySub()");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x8c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xc0, 0x0c, 0x00, 0x00, // next sub
            0x0a, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x00, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x00, // var count
            0x00, // vararg
            0x07, // extra vis
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Friend Static Sub MySub()");

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x14, 0x00, 0x00, 0x00, 0x00, // sub
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xe8, 0x00, 0x00, 0x00, // next sub
            0x09, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x00, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x00, // var count
            0x00, // vararg
            0x03, // extra vis
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Public Sub MySub()");

        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x8c, 0x11, // func flags
            0x2c, 0x02, // func nameid
            0xe8, 0x00, 0x00, 0x00, // next sub
            0x09, 0x00, // ord
            // unks
            0x04, 0x60,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret_bltin_or_offset
            0x00, 0x0e, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x00, // var count
            0x00, // vararg
            0x03, // extra vis
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Public Static Sub MySub()");
        Ok(())
    }

    #[test]
    fn declare() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x116, "SetLastError".to_string());
        pc.string_table.insert(0x117, "dwErrCode".to_string());
        pc.string_table.insert(0x119, "kernel32".to_string());
        pc.string_table.insert(0x11b, "GetLastError".to_string());
        pc.string_table.insert(0x11c, "ByOrd".to_string());
        pc.string_table.insert(0x11e, "beep".to_string());
        pc.string_table.insert(0x11f, "uType".to_string());
        pc.string_table.insert(0x11d, "MyDLL".to_string());
        pc.string_table.insert(0x120, "user32".to_string());
        let mut d = DecTester::new(&pc);
        d.set_imptbl(&[
            0x00, 0x00, 0x32, 0x02, 0x20, 0x00, 0x6f, 0x6f, 0xff, 0xff, 0x3a, 0x3a, 0x4e, 0x74,
            0x43, 0x6c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x53, 0x65, 0x74, 0x4c, 0x61, 0x73, 0x74, 0x45, 0x72, 0x72,
            0x6f, 0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x02, 0x50, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x47, 0x65, 0x74, 0x4c,
            0x61, 0x73, 0x74, 0x45, 0x72, 0x72, 0x6f, 0x72, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x3a, 0x02, 0x39, 0x05, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x02, 0xa0, 0x00, 0x00, 0x00, 0x60, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x42,
            0x65, 0x65, 0x70, 0x00,
        ]);

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x14, 0x00, 0x00, 0x00, 0x00, // sub definition
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0b, 0x12, // sub flags
            0x2c, 0x02, // sub nameid
            0x28, 0x01, 0x00, 0x00, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, // imp offset
            0xff, 0xff,
            0x01, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x04, // ret type
            0x01, 0x00, // var count
            0x22,
            0x00, 0x00, 0x00, 0x00,

            // Var
            0x69, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x03, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x80, 0x01, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(
            d.decompile(),
            "Public Declare PtrSafe Sub SetLastError Lib \"kernel32\" (dwErrCode As Long)"
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x08, 0x00, 0x00, 0x00, 0x00, // sub definition
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x2b, 0x12, // sub flags
            0x36, 0x02, // sub nameid
            0x90, 0x01, 0x00, 0x00, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0x03, 0x00, 0xff, 0xff, // ret bltin
            0x30, 0x00,  // imp offset
            0xff, 0xff,
            0x01, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x0c, // ret type
            0x00, 0x00, // var count
            0x22,
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(
            d.decompile(),
            "Declare PtrSafe Function GetLastError Lib \"kernel32\" () As Long"
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x04, 0x00, 0x00, 0x00, 0x00, // sub definition
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0b, 0x12, // sub flags
            0x38, 0x02, // sub nameid
            0x00, 0x02, 0x00, 0x00, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // first var offset
            0xff, 0xff, 0xff, 0xff,
            0x60, 0x00, // imp offset
            0xff, 0xff,
            0x01, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x04, // ret type
            0x00, 0x00, // var count
            0x22,
            0x00, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(
            d.decompile(),
            "Declare PtrSafe Sub ByOrd Lib \"MyDLL\" Alias \"#1337\" ()"
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x08, 0x00, 0x00, 0x00, 0x00, // sub definition
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x2b, 0x12, // sub flags
            0x3c, 0x02, // sub nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0x0b, 0x00, 0xff, 0xff, // ret bltin
            0x80, 0x00, // imp offset
            0xff, 0xff,
            0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x0c, // ret type
            0x01, 0x00, // var count
            0x22,
            0x00, 0x00, 0x00, 0x00,

            // Var
            0x69, 0x83, // flags
            0x3e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x03, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x80, 0x01, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(
            d.decompile(),
            "Declare PtrSafe Function beep Lib \"user32\" Alias \"MessageBeep\" (uType As Long) As Boolean"
        );
        Ok(())
    }

    #[test]
    fn enders() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        for op in [
            (0x67, "End"),
            (0x69, "End Function"),
            (0x6d, "End Property"),
            (0x6f, "End Sub"),
            (0x7a, "Exit Function"),
            (0x7b, "Exit Property"),
            (0x7c, "Exit Sub"),
        ] {
            d.set_code(&[op.0, 0x00]);
            assert_eq!(d.decompile(), op.1);
        }
        Ok(())
    }
}

mod callproc {
    use super::*;

    #[test]
    fn args() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x119, "MsgBox".to_string());
        pc.string_table.insert(0x11a, "MyMessage".to_string());
        pc.string_table.insert(0x11b, "MyTitle".to_string());
        pc.string_table.insert(0x11c, "Title".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x34, 0x02, // MyMessage
            0xd3, 0x00, // empty arg
            0x20, 0x00, 0x36, 0x02, // MyTitle
            0x41, 0x40, 0x32, 0x02, 0x03, 0x00 // call
        ]);
        assert_eq!(d.decompile(), format!("MsgBox MyMessage, , MyTitle"));

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x04, 0x00, 0x74, 0x65, 0x78, 0x74, // "text"
            0xb9, 0x00, 0x05, 0x00, 0x74, 0x69, 0x74, 0x6c, 0x65, 0x00, // "title"
            0xd4, 0x00, 0x38, 0x02, // Title:=
            0x41, 0x40, 0x32, 0x02, 0x02, 0x00 // call
        ]);
        assert_eq!(d.decompile(), format!("MsgBox \"text\", Title:=\"title\""));
        Ok(())
    }

    #[test]
    fn mods() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x119, "MyProc".to_string());
        pc.string_table.insert(0x11a, "Arg1".to_string());
        pc.string_table.insert(0x11b, "Arg2".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x41, 0x00, 0x32, 0x02, 0x00, 0x00 // call
        ]);
        assert_eq!(d.decompile(), format!("Call MyProc"));

        #[rustfmt::skip]
        let mut codebuf: [u8; 14] = [
            0x20, 0x00, 0x34, 0x02, // Arg1
            0x20, 0x00, 0x36, 0x02, // Arg2
            0x41, 0x00, 0x32, 0x02, 0x02, 0x00 // call
        ];
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("Call MyProc(Arg1, Arg2)"));

        codebuf[9] = 0b00_0001 << 2;
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("Call MyProc^(Arg1, Arg2)"));

        codebuf[9] = 0b00_0010 << 2;
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("Call MyProc%(Arg1, Arg2)"));

        codebuf[9] = 0b01_0011 << 2;
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("MyProc& Arg1, Arg2"));

        codebuf[9] = 0b01_0100 << 2;
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("MyProc! Arg1, Arg2"));

        codebuf[9] = 0b10_0101 << 2;
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("Call [MyProc]#(Arg1, Arg2)"));

        codebuf[9] = 0b10_0110 << 2;
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("Call [MyProc]@(Arg1, Arg2)"));

        codebuf[9] = 0b11_1000 << 2;
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("[MyProc]$ Arg1, Arg2"));
        Ok(())
    }

    #[test]
    fn callfn_assign() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x03, 0x00, 0x31, 0x32, 0x33, 0x00, // "123"
            0xac, 0x00, 0x01, 0x00, // 1
            0x24, 0x20, 0xdc, 0x00, 0x02, 0x00, // Left
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = Left$(\"123\", 1)"));
        Ok(())
    }
}

mod objects {
    use super::*;

    #[test]
    fn method() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x122, "MyDoc".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x44, 0x02, // MyDoc
            0x42, 0x40, 0x42, 0x00, 0x00, 0x00 // .Close no args
        ]);
        assert_eq!(d.decompile(), format!("MyDoc.Close"));
        Ok(())
    }

    #[test]
    fn prop_assign() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x123, "Documents".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        let mut codebuf: [u8; 24] = [
            0xb9, 0x00, 0x05, 0x00, 0x4d, 0x79, 0x44, 0x6f, 0x63, 0x00, // "MyDoc"
            0xac, 0x00, 0x01, 0x00, // 1
            0x24, 0x00, 0x46, 0x02, 0x01, 0x00, // Documents(...)
            0x28, 0x00, 0x06, 0x01, // assign
        ];
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), "Documents(1).Name = \"MyDoc\"".to_string());
        codebuf[20] = 0x2f;
        d.set_code(&codebuf);
        assert_eq!(
            d.decompile(),
            "Set Documents(1).Name = \"MyDoc\"".to_string()
        );
        Ok(())
    }

    #[test]
    fn func_and_propget() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x127, "ActiveDocument".to_string());
        pc.string_table.insert(0x128, "Shapes".to_string());
        pc.string_table.insert(0x119, "Title".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x20, 0x00, 0x4e, 0x02, // ActiveDocument
            0x25, 0x00, 0x50, 0x02, 0x01, 0x00, // Shapes(1)
            0x21, 0x00, 0x32, 0x02, // Title
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = ActiveDocument.Shapes(1).Title")
        );
        Ok(())
    }

    #[test]
    fn with() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x13a, "ActiveDocument".to_string());
        pc.string_table.insert(0x13c, "Bold".to_string());
        pc.string_table
            .insert(0x141, "RemovePersonalInformation".to_string());
        pc.string_table.insert(0x142, "Content".to_string());
        pc.string_table.insert(0x146, "ApplyTheme".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x04, 0x01, // start with
            0x20, 0x00, 0x74, 0x02, // obj
            0xf8, 0x00, // With
        ]);
        assert_eq!(d.decompile(), format!("With ActiveDocument"));

        #[rustfmt::skip]
        d.set_code(&[
            0x43, 0x40, 0x82, 0x02, 0x00, 0x00 // .method
        ]);
        assert_eq!(d.decompile(), format!(".RemovePersonalInformation"));

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x07, 0x00, 0x4d, 0x79, 0x54, 0x68, 0x65, 0x6d, 0x65, 0x00, // "MyTheme"
            0x43, 0x40, 0x8c, 0x02, 0x01, 0x00 // .method
        ]);
        assert_eq!(d.decompile(), format!(".ApplyTheme \"MyTheme\""));

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x07, 0x00, 0x4d, 0x79, 0x54, 0x68, 0x65, 0x6d, 0x65, 0x00, // "MyTheme"
            0x37, 0x00, 0x8c, 0x02, 0x01, 0x00, // ApplyTheme()
            0x27, 0x00, 0x04, 0x02, // var_blin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = .ApplyTheme(\"MyTheme\")")
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x04, 0x01, // start with
            0x35, 0x00, 0x84, 0x02, // .content
            0xf8, 0x00, // with
        ]);
        assert_eq!(d.decompile(), format!("With .Content"));

        #[rustfmt::skip]
        let codebuf: &mut[u8] = &mut [
            0xba, 0x04, // True
            0x39, 0x00, 0x78, 0x02, // .bold
        ];
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!(".Bold = True"));

        codebuf[2] = 0x3d;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("Set .Bold = True"));

        codebuf[2] = 0x3a;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("!Bold = True"));

        codebuf[2] = 0x3e;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("Set !Bold = True"));

        #[rustfmt::skip]
        let codebuf: &mut [u8] = &mut [
            0xac, 0x00, 0x02, 0x00, // 2
            0xac, 0x00, 0x01, 0x00, // 1
            0x3b, 0x00, 0x8c, 0x02, 0x01, 0x00, // ApplyTheme()
        ];
        d.set_code(codebuf);
        assert_eq!(d.decompile(), ".ApplyTheme(1) = 2".to_string());

        codebuf[8] = 0x3f;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), "Set .ApplyTheme(1) = 2".to_string());

        codebuf[8] = 0x3c;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), "!ApplyTheme(1) = 2".to_string());

        codebuf[8] = 0x40;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), "Set !ApplyTheme(1) = 2".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x71, 0x00, // end with
        ]);
        assert_eq!(d.decompile(), format!("End With"));
        Ok(())
    }

    #[test]
    fn bang() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x146, "Bang".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x0a, 0x02, // var_obj
            0x22, 0x00, 0x8c, 0x02, // Bang
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = var_obj!Bang"));

        #[rustfmt::skip]
        d.set_code(&[
            0x36, 0x00, 0x8c, 0x02, // Bang
            0x2e, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("Set var_bltin = !Bang"));

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x03, 0x00, 0x31, 0x32, 0x33, 0x00, // "123"
            0x20, 0x00, 0x0a, 0x02, // var_obj
            0x26, 0x00, 0x8c, 0x02, 0x01, 0x00, // Bang
            0x27, 0x00, 0x04, 0x02 // var_bltin
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = var_obj!Bang(\"123\")"));

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x03, 0x00, 0x31, 0x32, 0x33, 0x00, // "123"
            0x38, 0x00, 0x8c, 0x02, 0x01, 0x00, // Bang
            0x27, 0x00, 0x04, 0x02 // var_bltin
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = !Bang(\"123\")"));

        #[rustfmt::skip]
        let codebuf: &mut [u8] = &mut [
            0xac, 0x00, 0x2a, 0x00, // 42
            0x20, 0x00, 0x0a, 0x02, // var_obj
            0x29, 0x00, 0x8c, 0x02 // Bang
        ];
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("var_obj!Bang = 42"));

        codebuf[8] = 0x30;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("Set var_obj!Bang = 42"));

        #[rustfmt::skip]
        let codebuf: &mut [u8] = &mut [
            0xac, 0x00, 0x14, 0x00, // 20
            0xac, 0x00, 0x0a, 0x00, // 10
            0x20, 0x00, 0x0a, 0x02, // var_obj
            0x2d, 0x00, 0x8c, 0x02, 0x01, 0x00 // Bang
        ];
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("var_obj!Bang(10) = 20"));

        codebuf[12] = 0x34;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("Set var_obj!Bang(10) = 20"));
        Ok(())
    }

    #[test]
    fn set() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x129, "CreateObject".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xf0, 0x00, // Set mark
            0xb9, 0x00, 0x0b, 0x00, 0x45, 0x78, 0x63, 0x65, // Excel
            0x6c, 0x2e, 0x53, 0x68, 0x65, 0x65, 0x74, 0x00, // .Sheet
            0x24, 0x00, 0x52, 0x02, 0x01, 0x00, // CreateObject
            0x2e, 0x00, (OBJ.0<<1) as u8, (OBJ.0>>7) as u8 // Set =
        ]);
        assert_eq!(
            d.decompile(),
            format!("Set var_obj = CreateObject(\"Excel.Sheet\")")
        );

        #[rustfmt::skip]
        d.set_code(&[
            0xf0, 0x00, // Set mark
            0xb2, 0x00,
            0x2e, 0x00, (OBJ.0<<1) as u8, (OBJ.0>>7) as u8 // Set =
        ]);
        assert_eq!(d.decompile(), format!("Set var_obj = Nothing"));
        Ok(())
    }

    #[test]
    fn new() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);
        d.add_type(0x4, "Application");

        #[rustfmt::skip]
        d.set_code(&[
            0xc9, 0x00, 0x20, 0x00, // New Application
            0x27, 0x00, (OBJ.0<<1) as u8, (OBJ.0>>7) as u8 // var_obj =
        ]);
        assert_eq!(d.decompile(), format!("var_obj = New Application"));
        Ok(())
    }

    #[test]
    fn assign_args() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x11b, "ActiveDocument".to_string());
        pc.string_table.insert(0x11c, "Range".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        let codebuf: &mut [u8] = &mut [
            0xb9, 0x00, 0x03, 0x00, 0x78, 0x78, 0x78, 0x00, // "xxx"
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x03, 0x00, // 3
            0x20, 0x00, 0x36, 0x02, // ActiveDocument
            0x2c, 0x00, 0x38, 0x02, 0x02, 0x00, // .Range(...)
        ];
        d.set_code(codebuf);
        assert_eq!(
            d.decompile(),
            "ActiveDocument.Range(1, 3) = \"xxx\"".to_string()
        );

        codebuf[20] = 0x33;
        d.set_code(codebuf);
        assert_eq!(
            d.decompile(),
            "Set ActiveDocument.Range(1, 3) = \"xxx\"".to_string()
        );
        Ok(())
    }

    #[test]
    fn type_of() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);
        d.add_type(0x4, "Application");

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0x9d, 0x00, 0x20, 0x00, // Is Application
            0x15, 0x00, // Not
            0x9c, 0x00, // If
        ]);
        assert_eq!(
            d.decompile(),
            format!("If Not TypeOf var_bltin Is Application Then")
        );
        Ok(())
    }
}

mod arrays {
    use super::*;

    #[test]
    fn set() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        let codebuf: &mut [u8] = &mut [
            0xb9, 0x00, 0x01, 0x00, 0x41, 0x00, // "1"
            0xac, 0x00, 0x01, 0x00, // 1
            0x2b, 0x00, 0x04, 0x02, 0x01, 0x00
        ];
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("var_bltin(1) = \"A\""));

        codebuf[10] = 0x32;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("Set var_bltin(1) = \"A\""));
        Ok(())
    }

    #[test]
    fn get() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x11d, "MyArray".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x2a, 0x00, // 42
            0x24, 0x00, 0x3a, 0x02, 0x01, 0x00, // MyArray
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = MyArray(42)"));
        Ok(())
    }

    #[test]
    fn init() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x02, 0x00, // 2
            0xac, 0x00, 0x03, 0x00, // 3
            0x44, 0x00, 0x12, 0x00, 0x03, 0x00, // Array(3 args)
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = Array(1, 2, 3)"));
        Ok(())
    }

    #[test]
    fn index() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x03, 0x00, // 3
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x05, 0x00, // 5
            0x44, 0x00, 0x12, 0x00, 0x02, 0x00, // Array(2 args)
            0x23, 0x00, 0x01, 0x00, // index
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = Array(1, 5)(3)"));
        Ok(())
    }

    #[test]
    fn index_assign() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x11d, "MyFunc".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        let mut codebuf: [u8; 32] = [
            0xf0, 0x00, // Set mark
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x02, 0x00, // 2
            0xac, 0x00, 0x03, 0x00, // 3
            0xac, 0x00, 0x0a, 0x00, // 10
            0x24, 0x00, 0x3a, 0x02, 0x01, 0x00, // MyArray
            0x2a, 0x00, 0x03, 0x00, // assign
        ];
        d.set_code(&codebuf);
        assert_eq!(d.decompile(), format!("MyFunc(10)(1, 2, 3) = var_bltin"));

        codebuf[28] = 0x31;
        d.set_code(&codebuf);
        assert_eq!(
            d.decompile(),
            format!("Set MyFunc(10)(1, 2, 3) = var_bltin")
        );
        Ok(())
    }

    #[test]
    fn bound() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var
            0xac, 0x00, 0x01, 0x00, // 1
            0x8a, 0x00, 0x01, 0x00, // bound
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = LBound(var_bltin, 1)"));

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var
            0x91, 0x00, 0x00, 0x00, // bound
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = UBound(var_bltin)"));
        Ok(())
    }

    #[test]
    fn len() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x04, 0x00, 0x74, 0x65, 0x78, 0x74, // "text"
            0x1b, 0x00, // Len
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = Len(\"text\")"));

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x03, 0x00, 0x62, 0x69, 0x6e, 0x00, // "bin"
            0x1c, 0x00, // Len
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = LenB(\"bin\")"));
        Ok(())
    }

    #[test]
    fn erase() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x11d, "A".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x3a, 0x02, // A
            0x20, 0x00, 0x18, 0x00, // B
            0x72, 0x00, 0x02, 0x00, // erase
        ]);
        assert_eq!(d.decompile(), format!("Erase A, B"));
        Ok(())
    }

    #[test]
    fn paren() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x03, 0x00, // 3
            0xac, 0x00, 0x05, 0x00, // 5
            0x0b, 0x00, // +
            0x1d, 0x00, // ()
            0xac, 0x00, 0x02, 0x00, // 2
            0x10, 0x00, // /
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = (3 + 5) / 2"));
        Ok(())
    }
}

mod loops {
    use super::*;

    #[test]
    fn do_loop() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x0a, 0x00, // 10
            0x0a, 0x00, // >
            0x62, 0x00 // Do While
        ]);
        assert_eq!(d.decompile(), format!("Do While var_bltin > 10"));

        #[rustfmt::skip]
        d.set_code(&[
            0xbc, 0x00 // Loop
        ]);
        assert_eq!(d.decompile(), format!("Loop"));

        #[rustfmt::skip]
        d.set_code(&[
            0x5f, 0x00 // Do
        ]);
        assert_eq!(d.decompile(), format!("Do"));

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x0a, 0x00, // 10
            0x0a, 0x00, // >
            0xbe, 0x00 // Loop While
        ]);
        assert_eq!(d.decompile(), format!("Loop While var_bltin > 10"));

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x0a, 0x00, // 10
            0x05, 0x00, // =
            0x61, 0x00 // Do Until
        ]);
        assert_eq!(d.decompile(), format!("Do Until var_bltin = 10"));

        #[rustfmt::skip]
        d.set_code(&[
            0x78, 0x00
        ]);
        assert_eq!(d.decompile(), format!("Exit Do"));
        Ok(())
    }

    #[test]
    fn for_each() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x123, "MyArray".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x02, 0x01, // begin var
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0x01, 0x01, // end var
            0x20, 0x00, 0x46, 0x02, // Array
            0x93, 0x00 // For Each
        ]);
        assert_eq!(d.decompile(), format!("For Each var_bltin In MyArray"));

        #[rustfmt::skip]
        d.set_code(&[
            0x02, 0x01, // begin var
            0xca, 0x00 // Next
        ]);
        assert_eq!(d.decompile(), format!("Next"));

        d.set_code(&[
            0x02, 0x01, // begin var
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0x01, 0x01, // end var
            0xcb, 0x00, // Next var
        ]);
        assert_eq!(d.decompile(), format!("Next var_bltin"));

        d.set_code(&[
            0x02, 0x01, // begin var
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0x01, 0x01, // end var
            0xac, 0x00, 0x10, 0x00, // 16
            0xac, 0x00, 0x02, 0x00, // 2
            0xac, 0x00, 0x01, 0x00, // 1
            0x16, 0x00, // -
            0x95, 0x00, // For
        ]);
        assert_eq!(d.decompile(), format!("For var_bltin = 16 To 2 Step -1"));

        #[rustfmt::skip]
        d.set_code(&[
            0x79, 0x00
        ]);
        assert_eq!(d.decompile(), format!("Exit For"));
        Ok(())
    }

    #[test]
    fn while_whend() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x0a, 0x00, // 10
            0x09, 0x00, // <
            0xf7, 0x00 // While
        ]);
        assert_eq!(d.decompile(), format!("While var_bltin < 10"));

        #[rustfmt::skip]
        d.set_code(&[
            0xf6, 0x00, // Wend
        ]);
        assert_eq!(d.decompile(), format!("Wend"));
        Ok(())
    }
}

mod conds {
    use super::*;

    #[test]
    fn multiline_if() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x0a, 0x00, // 10
            0x0a, 0x00, // >
            0x9c, 0x00 // If
        ]);
        assert_eq!(d.decompile(), format!("If var_bltin > 10 Then"));

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x0a, 0x00, // 100
            0x09, 0x00, // <
            0x65, 0x00 // ElseIf
        ]);
        assert_eq!(d.decompile(), format!("ElseIf var_bltin < 10 Then"));

        #[rustfmt::skip]
        d.set_code(&[
            0x64, 0x00 // Else
        ]);
        assert_eq!(d.decompile(), format!("Else"));

        #[rustfmt::skip]
        d.set_code(&[
            0x6b, 0x00 // End If
        ]);
        assert_eq!(d.decompile(), format!("End If"));
        Ok(())
    }

    #[test]
    fn inline_if() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x12e, "result".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x00, 0x00, // 0
            0x0a, 0x00, // >
            0x9b, 0x00, // inline If
            0x47, 0x00, // inline Then
            0xba, 0x04, // True
            0x27, 0x00, 0x5c, 0x02, // set result
            0x63, 0x00, // inline Else
            0x47, 0x00, // inline Then
            0xba, 0x00, // False
            0x27, 0x00, 0x5c, 0x02, // set result
            0x6a, 0x00 // inline End If
        ]);
        assert_eq!(
            d.decompile(),
            format!("If var_bltin > 0 Then result = True Else result = False")
        );
        Ok(())
    }

    #[test]
    fn select_case() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xed, 0x00, // Select Case
        ]);
        assert_eq!(d.decompile(), format!("Select Case var_bltin"));

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x4b, 0x00, // case item marker
            0x54, 0x00, // Case
        ]);
        assert_eq!(d.decompile(), format!("Case 1"));

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x02, 0x00, // 2
            0x4b, 0x00, // case item marker
            0xac, 0x00, 0x04, 0x00, // 4
            0xac, 0x00, 0x06, 0x00, // 6
            0x4c, 0x00, // case item to item marker
            0x54, 0x00, // Case
        ]);
        assert_eq!(d.decompile(), format!("Case 2, 4 To 6"));

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x00, 0x00, // 0
            0x4e, 0x00, // <
            0xac, 0x00, 0x01, 0x00, // 1
            0x4d, 0x00, // >
            0xac, 0x00, 0x02, 0x00, // 2
            0x50, 0x00, // <=
            0xac, 0x00, 0x03, 0x00, // 3
            0x4f, 0x00, // >=
            0xac, 0x00, 0x04, 0x00, // 4
            0x51, 0x00, // <>
            0xac, 0x00, 0x05, 0x00, // 5
            0x52, 0x00, // =
            0x54, 0x00, // Case
        ]);
        assert_eq!(
            d.decompile(),
            format!("Case Is < 0, Is > 1, Is <= 2, Is >= 3, Is <> 4, Is = 5")
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x53, 0x00, // Case Else
        ]);
        assert_eq!(d.decompile(), format!("Case Else"));

        #[rustfmt::skip]
        d.set_code(&[
            0x6e, 0x00, // End Select
        ]);
        assert_eq!(d.decompile(), format!("End Select"));
        Ok(())
    }

    #[test]
    fn multi_stmt_select() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x00, 0x00, // 0
            0x27, 0x00, 0x04, 0x02, // var_bltin =
            0x46, 0x00, 0x00, 0x00, // :
            0x20, 0x00, 0x08, 0x02, // var_noas
            0xac, 0x00, 0x02, 0x00, // 2
            0x0f, 0x00, // *
            0xed, 0x00, // Select
            0x46, 0x00, 0x00, 0x00, // :
            0xac, 0x00, 0x00, 0x00, // 0
            0x4b, 0x00, // Case item (expr)
            0xac, 0x00, 0x02, 0x00, // 2
            0xac, 0x00, 0x03, 0x00, // 3
            0x4c, 0x00, // Case item (e2e)
            0x54, 0x00, // Case stmt
            0x46, 0x00, 0x00, 0x00, // :
            0xac, 0x00, 0x01, 0x00, // 1
            0x27, 0x00, 0x04, 0x02, // var_bltin =
            0x46, 0x00, 0x00, 0x00, // :
            0xac, 0x00, 0x04, 0x00, // 4
            0x4d, 0x00, // Case item (is)
            0xac, 0x00, 0x0a, 0x00, // 10
            0x4e, 0x00, // Case item (is)
            0x54, 0x00, // Case stmt
            0x46, 0x00, 0x00, 0x00, // :
            0xac, 0x00, 0x02, 0x00, // 2
            0x27, 0x00, 0x04, 0x02, // var_bltin =
            0x46, 0x00, 0x00, 0x00, // :
            0x53, 0x00, // Case Else
            0x46, 0x00, 0x00, 0x00, // :
            0xac, 0x00, 0x03, 0x00, // 3
            0x27, 0x00, 0x04, 0x02, // var_bltin =
            0x46, 0x00, 0x00, 0x00, // :
            0x6e, 0x00, // End Select
            0x46, 0x00, 0x00, 0x00, // :
            0xac, 0x00, 0x04, 0x00, // 4
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!(
            "var_bltin = 0: Select Case var_noas * 2: Case 0, 2 To 3: var_bltin = 1: Case Is > 4, Is < 10: var_bltin = 2: Case Else: var_bltin = 3: End Select: var_bltin = 4"
        ));
        Ok(())
    }
}

mod funcs {
    use super::*;

    #[test]
    fn conv() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        for conv in [
            (0x2c, "CBool"),
            (0x44, "CByte"),
            (0x18, "CCur"),
            (0x1c, "CDate"),
            (0x14, "CDbl"),
            (0x08, "CInt"),
            (0x0c, "CLng"),
            (0x34, "CLngLng"),
            (0x04, "CLngPtr"),
            (0x10, "CSng"),
            (0x20, "CStr"),
            (0x00, "CVar"),
        ] {
            #[rustfmt::skip]
            d.set_code(&[
                0xac, 0x00, 0x01, 0x00, // 1
                0x58, conv.0, // cxx
                0x27, 0x00, 0x04, 0x02, // var_bltin =
            ]);
            assert_eq!(d.decompile(), format!("var_bltin = {}(1)", conv.1));
        }

        #[rustfmt::skip]
        let codebuf: &mut [u8] = &mut [
            0xac, 0x00, 0x01, 0x00, // 1
            0x59, 0x00, // cxx
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ];
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("var_bltin = CVDate(1)"));

        codebuf[5] = 0x28;
        d.set_code(codebuf);
        assert_eq!(d.decompile(), format!("var_bltin = CVErr(1)"));
        Ok(())
    }

    #[test]
    fn instr() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x147, "vbTextCompare".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x08, 0x00, 0x68, 0x61, 0x79, 0x73, 0x74, 0x61, 0x63, 0x6b, // heystack
            0xb9, 0x00, 0x06, 0x00, 0x6e, 0x65, 0x65, 0x64, 0x6c, 0x65, // needle
            0x84, 0x00, // InStr
            0x27, 0x00, 0x04, 0x02 // var_bltin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = InStr(\"haystack\", \"needle\")")
        );

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x08, 0x00, 0x68, 0x61, 0x79, 0x73, 0x74, 0x61, 0x63, 0x6b, // heystack
            0xb9, 0x00, 0x06, 0x00, 0x6e, 0x65, 0x65, 0x64, 0x6c, 0x65, // needle
            0x87, 0x00, // InStr
            0x27, 0x00, 0x04, 0x02 // var_bltin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = InStrB(\"haystack\", \"needle\")")
        );

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0xb9, 0x00, 0x08, 0x00, 0x68, 0x61, 0x79, 0x73, 0x74, 0x61, 0x63, 0x6b, // heystack
            0xb9, 0x00, 0x06, 0x00, 0x6e, 0x65, 0x65, 0x64, 0x6c, 0x65, // needle
            0x85, 0x00, // InStr
            0x27, 0x00, 0x04, 0x02 // var_bltin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = InStr(1, \"haystack\", \"needle\")")
        );

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0xb9, 0x00, 0x08, 0x00, 0x68, 0x61, 0x79, 0x73, 0x74, 0x61, 0x63, 0x6b, // heystack
            0xb9, 0x00, 0x06, 0x00, 0x6e, 0x65, 0x65, 0x64, 0x6c, 0x65, // needle
            0x88, 0x00, // InStr
            0x27, 0x00, 0x04, 0x02 // var_bltin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = InStrB(1, \"haystack\", \"needle\")")
        );

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0xb9, 0x00, 0x08, 0x00, 0x68, 0x61, 0x79, 0x73, 0x74, 0x61, 0x63, 0x6b, // heystack
            0xb9, 0x00, 0x06, 0x00, 0x6e, 0x65, 0x65, 0x64, 0x6c, 0x65, // needle
            0x20, 0x00, 0x8e, 0x02, // vbTextCompare
            0x86, 0x00, // InStr
            0x27, 0x00, 0x04, 0x02 // var_bltin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = InStr(1, \"haystack\", \"needle\", vbTextCompare)")
        );

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0xb9, 0x00, 0x08, 0x00, 0x68, 0x61, 0x79, 0x73, 0x74, 0x61, 0x63, 0x6b, // heystack
            0xb9, 0x00, 0x06, 0x00, 0x6e, 0x65, 0x65, 0x64, 0x6c, 0x65, // needle
            0x20, 0x00, 0x8e, 0x02, // vbTextCompare
            0x89, 0x00, // InStr
            0x27, 0x00, 0x04, 0x02 // var_bltin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = InStrB(1, \"haystack\", \"needle\", vbTextCompare)")
        );
        Ok(())
    }

    #[test]
    fn math() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        let codebuf: &mut [u8] = &mut [
            0xac, 0x00, 0x01, 0x00, // 1
            0x00, 0x00, // fn
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ];
        for conv in [(0x17, "Abs"), (0x18, "Fix"), (0x19, "Int"), (0x1a, "Sgn")] {
            codebuf[4] = conv.0;
            d.set_code(codebuf);
            assert_eq!(d.decompile(), format!("var_bltin = {}(1)", conv.1));
        }
        Ok(())
    }

    #[test]
    fn strcomp() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x147, "vbTextCompare".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x01, 0x00, 0x61, 0x00, // "a"
            0xb9, 0x00, 0x01, 0x00, 0x62, 0x00, // "b"
            0x8d, 0x00, // strcomp(2)
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = StrComp(\"a\", \"b\")"));

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x01, 0x00, 0x61, 0x00, // "a"
            0xb9, 0x00, 0x01, 0x00, 0x62, 0x00, // "b"
            0x20, 0x00, 0x8e, 0x02, // vbTextCompare
            0x8e, 0x00, // strcomp(3)
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(
            d.decompile(),
            format!("var_bltin = StrComp(\"a\", \"b\", vbTextCompare)")
        );
        Ok(())
    }
}

mod ops {
    use super::*;

    #[test]
    fn two_operand() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x194, "expr1".to_string());
        pc.string_table.insert(0x195, "expr2".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        let mut codebuf: [u8; 16] = [
            0xa4, 0x00, // Let
            0x20, 0x00, 0x28, 0x03, // expr1
            0x20, 0x00, 0x2a, 0x03, // expr2
            0x00, 0x00, // op
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ];

        for op in [
            (0x0f, "*"),
            (0x13, "^"),
            (0x10, "/"),
            (0x0e, "\\"),
            (0x0d, "Mod"),
            (0x0b, "+"),
            (0x0c, "-"),
            (0x05, "="),
            (0x14, "Is"),
            (0x12, "Like"),
            (0x11, "&"),
            (0x04, "And"),
            (0x01, "Eqv"),
            (0x03, "Or"),
            (0x02, "Xor"),
        ] {
            codebuf[10] = op.0;
            d.set_code(&codebuf);
            assert_eq!(
                d.decompile(),
                format!("Let var_bltin = expr1 {} expr2", op.1)
            );
        }
        Ok(())
    }

    #[test]
    fn single_operand() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // expr1
            0x15, 0x00, // op
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = Not var_bltin"));

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x04, 0x02, // expr1
            0x16, 0x00, // op
            0x27, 0x00, 0x04, 0x02, // var_bltin =
        ]);
        assert_eq!(d.decompile(), format!("var_bltin = -var_bltin"));
        Ok(())
    }

    #[test]
    fn xset() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x130, "var1".to_string());
        pc.string_table.insert(0x134, "var2".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x02, 0x00, 0x3c, 0x3c, // "<<"
            0x20, 0x00, 0x60, 0x02, // var1
            0xbf, 0x00, // lset
        ]);
        assert_eq!(d.decompile(), format!("LSet var1 = \"<<\""));

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x68, 0x02, // var2
            0x20, 0x00, 0x60, 0x02, // var1
            0xea, 0x00, // rset
        ]);
        assert_eq!(d.decompile(), format!("RSet var1 = var2"));
        Ok(())
    }

    #[test]
    fn name() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x03, 0x00, 0x6f, 0x6c, 0x64, 0x00,
            0xb9, 0x00, 0x03, 0x00, 0x6e, 0x65, 0x77, 0x00,
            0xc8, 0x00
        ]);
        assert_eq!(d.decompile(), format!("Name \"old\" As \"new\""));
        Ok(())
    }

    #[test]
    fn mid() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x01, 0x00, 0x61, 0x00, // "a"
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x04, // default
            0xc6, 0x00, // mid
        ]);
        assert_eq!(d.decompile(), "Mid(var_bltin, 1) = \"a\"".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x01, 0x00, 0x61, 0x00, // "a"
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x02, 0x00, // 2
            0xc6, 0x20, // mid
        ]);
        assert_eq!(d.decompile(), "Mid$(var_bltin, 1, 2) = \"a\"".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x01, 0x00, 0x61, 0x00, // "a"
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x02, 0x00, // 2
            0xc7, 0x00, // mid
        ]);
        assert_eq!(d.decompile(), "MidB(var_bltin, 1, 2) = \"a\"".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x01, 0x00, 0x61, 0x00, // "a"
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x04, // default
            0xc7, 0x20, // mid$
        ]);
        assert_eq!(d.decompile(), "MidB$(var_bltin, 1) = \"a\"".to_string());
        Ok(())
    }
}

mod type_enum {
    use super::*;

    #[test]
    fn enums() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x117, "MyEnum".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_functbl(&[
            // Public
            0x06, 0x10, // flags
            0x2e, 0x02, // id
            0x18, 0x00, 0x00, 0x00, // next
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x18, 0x00,
            0x01, 0x00, // vis
            0x00, 0x00, 0xff, 0xff,

            // Private
            0x06, 0x10, // flags
            0x2e, 0x02, // id
            0x30, 0x00, 0x00, 0x00, // next
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x20, 0x00,
            0x00, 0x00, // vis
            0x00, 0x00, 0xff, 0xff,

            // NoVis
            0x06, 0x10, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff, // next
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x28, 0x00,
            0x01, 0x00, // vis
            0x00, 0x00, 0xff, 0xff,
        ]);

        #[rustfmt::skip]
        d.set_code(&[
            0xf3, 0x0c, 0x00, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "Public Enum MyEnum".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xf3, 0x0c, 0x18, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "Private Enum MyEnum".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xf3, 0x08, 0x30, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "Enum MyEnum".to_string());
        Ok(())
    }

    #[test]
    fn types() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x117, "MyType".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_functbl(&[
            // Public
            0x06, 0x00, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff, // next
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x48, 0x00,
            0x01, 0x00, // vis
            0x00, 0x00, 0xff, 0xff,

            // Private
            0x06, 0x00, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff, // next
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x50, 0x00,
            0x00, 0x00, // vis
            0x00, 0x00, 0xff, 0xff,

            // NoVis
            0x06, 0x00, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff, // next
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x58, 0x00,
            0x01, 0x00, // vis
            0x00, 0x00, 0xff, 0xff,
        ]);

        #[rustfmt::skip]
        d.set_code(&[
            0xf3, 0x04, 0x00, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "Public Type MyType".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xf3, 0x04, 0x18, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "Private Type MyType".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xf3, 0x00, 0x30, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "Type MyType".to_string());
        Ok(())
    }

    #[test]
    fn enders() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x70, 0x00
        ]);
        assert_eq!(d.decompile(), "End Type".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x06, 0x01
        ]);
        assert_eq!(d.decompile(), "End Enum".to_string());
        Ok(())
    }
}

mod stmts {
    use super::*;

    #[test]
    fn error() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x6e, 0x00,
            0x73, 0x00
        ]);
        assert_eq!(d.decompile(), "Error 110".to_string());
        Ok(())
    }

    #[test]
    fn option() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        for op in [
            (0x00, "Option Base 0"),
            (0x04, "Option Base 1"),
            (0x08, "Option Compare Text"),
            (0x0c, "Option Compare Binary"),
            (0x10, "Option Explicit"),
            (0x14, "Option Private Module"),
            (0x1c, "Option Compare Database"),
        ] {
            #[rustfmt::skip]
            d.set_code(&[
                0xd0, op.0
            ]);
            assert_eq!(d.decompile(), op.1.to_string());
        }
        Ok(())
    }

    #[test]
    fn comments() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xe7, 0x00, 0x08, 0x00,
            0x20, 0x63, 0x6f, 0x6d, 0x6d, 0x65, 0x6e, 0x74
        ]);
        assert_eq!(d.decompile(), "Rem comment".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xe3, 0x00, 0x04, 0x00, 0x08, 0x00,
            0x20, 0x63, 0x6f, 0x6d, 0x6d, 0x65, 0x6e, 0x74
        ]);
        assert_eq!(d.decompile(), "   ' comment".to_string());
        Ok(())
    }

    #[test]
    fn non_compiled() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xe6, 0x00, 0x0c, 0x00,
            0x6e, 0x6f, 0x74, 0x2d, 0x63, 0x6f, 0x6d, 0x70, 0x69, 0x6c, 0x65, 0x64
        ]);
        assert_eq!(d.decompile(), "not-compiled".to_string());
        Ok(())
    }

    #[test]
    fn debug_assert() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x01, 0x00, // 1
            0x06, 0x00, // <>
            0x45, 0x00, // Assert
        ]);
        assert_eq!(d.decompile(), "Debug.Assert 1 <> 1".to_string());
        Ok(())
    }

    #[test]
    fn defxxx() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x08, 0x01, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "DefInt A".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x0c, 0x15, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "DefLng A, C, E".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x10, 0x37, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "DefSng A-C, E-F".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x14, 0xff, 0xfb, 0xff, 0x03
        ]);
        assert_eq!(d.decompile(), "DefDbl A-J, L-Z".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x18, 0x02, 0x00, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "DefCur B".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x1c, 0xf5, 0x04, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "DefDate A, C, E-H, K".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x20, 0x00, 0xfc, 0x7f, 0x01
        ]);
        assert_eq!(d.decompile(), "DefStr K-W, Y".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x24, 0x02, 0x42, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "DefObj B, J, O".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x2c, 0x00, 0x00, 0x02, 0x00
        ]);
        assert_eq!(d.decompile(), "DefBool R".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x30, 0x00, 0x00, 0x04, 0x00
        ]);
        assert_eq!(d.decompile(), "DefVar S".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x34, 0x00, 0x00, 0x08, 0x00
        ]);
        assert_eq!(d.decompile(), "DefLngLng T".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x44, 0x00, 0x00, 0x80, 0x01
        ]);
        assert_eq!(d.decompile(), "DefByte X-Y".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5c, 0x04, 0x00, 0x00, 0x01, 0x00
        ]);
        assert_eq!(d.decompile(), "DefLngPtr Q".to_string());
        Ok(())
    }
}

mod jumps {
    use super::*;

    #[test]
    fn jumps() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x12b, "MyLabel".to_string());
        pc.string_table.insert(0x11c, "10".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xa3, 0x00, 0x56, 0x02, // label
        ]);
        assert_eq!(d.decompile(), "MyLabel:".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xa8, 0x00, 0x38, 0x02, // line number
            0xe3, 0x00, 0x03, 0x00, // rem
            0x08, 0x00, 0x20, 0x6c, 0x69, 0x6e, 0x65, 0x20, 0x31, 0x30 // rem data
        ]);
        assert_eq!(d.decompile(), "10 ' line 10".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x99, 0x00, 0x56, 0x02,
        ]);
        assert_eq!(d.decompile(), "GoSub MyLabel".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x9a, 0x00, 0x56, 0x02,
        ]);
        assert_eq!(d.decompile(), "GoTo MyLabel".to_string());
        Ok(())
    }

    #[test]
    fn ret() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xe9, 0x00,
        ]);
        assert_eq!(d.decompile(), "Return".to_string());
        Ok(())
    }

    #[test]
    fn on_error() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x13d, "Label".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xcc, 0x00, 0x7a, 0x02
        ]);
        assert_eq!(d.decompile(), "On Error GoTo Label".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xcc, 0x04, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "On Error Resume Next".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xcc, 0x08, 0x00, 0x00
        ]);
        assert_eq!(d.decompile(), "On Error GoTo 0".to_string());
        Ok(())
    }

    #[test]
    fn on_cond() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x13c, "Cond".to_string());
        pc.string_table.insert(0x13d, "Label1".to_string());
        pc.string_table.insert(0x13f, "Label2".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x78, 0x02, // cond
            0xcd, 0x00, 0x02, 0x00, // On...
            0x7a, 0x02, // Label1
        ]);
        assert_eq!(d.decompile(), "On Cond GoSub Label1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x78, 0x02, // cond
            0xce, 0x00, 0x04, 0x00, // On...
            0x7a, 0x02, // Label1
            0x7e, 0x02, // Label2
        ]);
        assert_eq!(d.decompile(), "On Cond GoTo Label1, Label2".to_string());
        Ok(())
    }

    #[test]
    fn resume() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x118, "Label".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xe8, 0x20, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Resume".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xe8, 0x04, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Resume Next".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xe8, 0x08, 0x00, 0x00,
        ]);
        assert_eq!(d.decompile(), "Resume 0".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xe8, 0x00, 0x30, 0x02,
        ]);
        assert_eq!(d.decompile(), "Resume Label".to_string());
        Ok(())
    }

    #[test]
    fn stop() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xf2, 0x00,
        ]);
        assert_eq!(d.decompile(), "Stop".to_string());
        Ok(())
    }

    #[test]
    fn address_of() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x0122, "CalledProc".to_string());
        pc.string_table.insert(0x0117, "ArgProc".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x49, 0x00, 0x2e, 0x02, // AddressOf fn
            0x41, 0x40, 0x44, 0x02, 0x01, 0x00, // call proc
        ]);
        assert_eq!(d.decompile(), "CalledProc AddressOf ArgProc".to_string());
        Ok(())
    }

    #[test]
    fn multiline() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x0116, "A".to_string());
        pc.string_table.insert(0x0c, "B".to_string());
        pc.string_table.insert(0x0121, "C".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x27, 0x00, 0x2c, 0x02, // assign
            0x46, 0x00, 0x00, 0x00, // : no space
            0xac, 0x00, 0x02, 0x00, // 2
            0x27, 0x00, 0x18, 0x00, // assign
            0x46, 0x00, 0x12, 0x00, // : space
            0xac, 0x00, 0x03, 0x00, // 3
            0x27, 0x00, 0x42, 0x02, // assign
        ]);
        assert_eq!(d.decompile(), "A = 1: B = 2:     C = 3".to_string());
        Ok(())
    }
}

mod input_output {
    use super::*;

    #[test]
    fn open() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x0118, "MyFile".to_string());
        pc.string_table.insert(0x011d, "fileno".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0x1e, 0x00, // #
            0xab, 0x00, // default
            0xcf, 0x00, 0x04, 0x00 // Open
        ]);
        assert_eq!(d.decompile(), "Open MyFile For Random As #1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x20, 0x00 // Open
        ]);
        assert_eq!(d.decompile(), "Open MyFile For Binary As 1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x01, 0x00 // Open
        ]);
        assert_eq!(d.decompile(), "Open MyFile For Input As 1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x02, 0x00 // Open
        ]);
        assert_eq!(d.decompile(), "Open MyFile For Output As 1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x08, 0x00 // Open
        ]);
        assert_eq!(d.decompile(), "Open MyFile For Append As 1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x01, 0x01 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Input Access Read As 1".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x02, 0x02 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Output Access Write As 1".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x04, 0x03 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Random Access Read Write As 1".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x0a, 0x00, // len
            0xcf, 0x00, 0x04, 0x23 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Random Access Read Write Lock Write As 1 Len = 10".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x01, 0x20 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Input Lock Write As 1".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x02, 0x20 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Output Lock Write As 1".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x08, 0x40 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Append Shared As 1".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xcf, 0x00, 0x04, 0x30 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Random Lock Read As 1".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x0a, 0x00, // len
            0xcf, 0x00, 0x20, 0x00 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Binary As 1 Len = 10".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x30, 0x02, // MyFile
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x14, 0x00, // len
            0xcf, 0x00, 0x01, 0x30 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open MyFile For Input Lock Read As 1 Len = 20".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x06, 0x00, 0x6d, 0x79, 0x66, 0x69, 0x6c, 0x65, // "myfile"
            0x20, 0x00, 0x3a, 0x02, // fileno
            0xab, 0x00, // default
            0xcf, 0x00, 0x01, 0x00 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open \"myfile\" For Input As fileno".to_string()
        );

        #[rustfmt::skip]
        d.set_code(&[
            0xb9, 0x00, 0x06, 0x00, 0x6d, 0x79, 0x66, 0x69, 0x6c, 0x65, // "myfile"
            0x20, 0x00, 0x3a, 0x02, // fileno
            0x1e, 0x00, // #
            0xab, 0x00, // default
            0xcf, 0x00, 0x02, 0x20 // Open
        ]);
        assert_eq!(
            d.decompile(),
            "Open \"myfile\" For Output Lock Write As #fileno".to_string()
        );
        Ok(())
    }

    #[test]
    fn close() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x011d, "fileno".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x56, 0x00, 0x01, 0x00, // close
        ]);
        assert_eq!(d.decompile(), "Close 1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x02, 0x00, // 2
            0x1e, 0x00, // #
            0x56, 0x00, 0x01, 0x00, // close
        ]);
        assert_eq!(d.decompile(), "Close #2".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x3a, 0x02, // fileno
            0x56, 0x00, 0x01, 0x00, // close
        ]);
        assert_eq!(d.decompile(), "Close fileno".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x3a, 0x02, // fileno
            0x1e, 0x00, // #
            0x56, 0x00, 0x01, 0x00, // close
        ]);
        assert_eq!(d.decompile(), "Close #fileno".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x57, 0x00, // close
        ]);
        assert_eq!(d.decompile(), "Close".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x1e, 0x00, // #
            0xac, 0x00, 0x02, 0x00, // 2
            0x20, 0x00, 0x3a, 0x02, // fileno
            0x56, 0x00, 0x03, 0x00, // close
        ]);
        assert_eq!(d.decompile(), "Close #1, 2, fileno".to_string());
        Ok(())
    }

    #[test]
    fn get_put() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x011d, "fileno".to_string());
        pc.string_table.insert(0x011b, "result".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x04, 0x00, // 4
            0x1e, 0x00, // #
            0xab, 0x00, // default
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0x98, 0x00, // Get
        ]);
        assert_eq!(d.decompile(), "Get #4, , var_bltin".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x20, 0x00, 0x36, 0x02, // result
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xe2, 0x00, // put
        ]);
        assert_eq!(d.decompile(), "Put 1, result, var_bltin".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x3a, 0x02, // result
            0x1e, 0x00, // #
            0xab, 0x00, // default
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xe2, 0x00, // put
        ]);
        assert_eq!(d.decompile(), "Put #fileno, , var_bltin".to_string());
        Ok(())
    }

    #[test]
    fn seek() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x1e, 0x00, // #
            0x20, 0x00, 0x04, 0x02, // var_bltin
            0xec, 0x00, // seek
        ]);
        assert_eq!(d.decompile(), "Seek #1, var_bltin".to_string());
        Ok(())
    }

    #[test]
    fn lock_unlock() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0xab, 0x00, // default
            0xab, 0x00, // default
            0xbb, 0x0c, // Lock
        ]);
        assert_eq!(d.decompile(), "Lock 1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x02, 0x00, // 2
            0x1e, 0x00, // #
            0xac, 0x00, 0x0a, 0x00, // 10
            0xab, 0x00, // default
            0xbb, 0x08, // Lock
        ]);
        assert_eq!(d.decompile(), "Lock #2, 10".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x02, 0x00, // 2
            0x1e, 0x00, // #
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x0a, 0x00, // 10
            0xbb, 0x00, // Lock
        ]);
        assert_eq!(d.decompile(), "Lock #2, 1 To 10".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x02, 0x00, // 2
            0xab, 0x00, // default
            0xac, 0x00, 0x0a, 0x00, // 10
            0xf4, 0x04, // Unlock
        ]);
        assert_eq!(d.decompile(), "Unlock 2, To 10".to_string());
        Ok(())
    }

    #[test]
    fn input() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x011b, "a".to_string());
        pc.string_table.insert(0xc, "b".to_string());
        pc.string_table.insert(0x11c, "fileno".to_string());
        pc.string_table.insert(0x11d, "c".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x20, 0x00, 0x38, 0x02, // fileno
            0x1e, 0x00, // #
            0xa0, 0x00, // Input #
            0x20, 0x00, 0x36, 0x02, // a
            0xa2, 0x00, // sep
            0x20, 0x00, 0x18, 0x00, // b
            0xa2, 0x00, // sep
            0x20, 0x00, 0x3a, 0x02, // c
            0xa2, 0x00, // sep
            0xa1, 0x00, // end
        ]);
        assert_eq!(d.decompile(), "Input #fileno, a, b, c".to_string());
        Ok(())
    }

    #[test]
    fn line_input() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x0118, "MyLine".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x20, 0x00, 0x30, 0x02, // MyLine
            0xa7, 0x00, // Line Input #
        ]);
        assert_eq!(d.decompile(), "Line Input #1, MyLine".to_string());
        Ok(())
    }

    #[test]
    fn write() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x0118, "MyLine".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x1e, 0x00, // #
            0xf9, 0x00, // Write chan
            0xb9, 0x00, 0x03, 0x00, 0x31, 0x32, 0x33, 0x00, // "123"
            0xd9, 0x00, // Print terminator
        ]);
        assert_eq!(d.decompile(), "Write #1, \"123\"".to_string());
        Ok(())
    }

    #[test]
    fn print() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x0118, "MyLine".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x1e, 0x00, // #
            0xd5, 0x00, // Print chan
            0xdb, 0x00, // Print terminator
        ]);
        assert_eq!(d.decompile(), "Print #1,".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5b, 0x00, // Debug
            0xdc, 0x00, // Print
            0xdb, 0x00, // Print terminator
        ]);
        assert_eq!(d.decompile(), "Debug.Print".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0x5b, 0x00, // Debug
            0xdc, 0x00, // Print
            0xac, 0x00, 0x01, 0x00, // 1
            0xd9, 0x00, // Print terminator
        ]);
        assert_eq!(d.decompile(), "Debug.Print 1".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x1e, 0x00, // #
            0xd5, 0x00, // Print chan
            0xac, 0x00, 0x02, 0x00, // 2
            0xd8, 0x00, // sep
            0xd7, 0x00, // Print comma terminator
        ]);
        assert_eq!(d.decompile(), "Print #1, 2,".to_string());

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0x1e, 0x00, // #
            0xd5, 0x00, // Print chan
            0xba, 0x04, // True
            0xd8, 0x00, // sep
            0xd6, 0x00, // ,
            0xe0, 0x00, // Tab
            0xdd, 0x00, // ;
            0xdd, 0x00, // ;
            0xac, 0x00, 0x01, 0x00, // 1
            0xda, 0x00, // ; mod
            0xac, 0x00, 0x01, 0x00, // 1
            0xde, 0x00, // Spc()
            0xd6, 0x00, // ,
            0xac, 0x00, 0x02, 0x00, // 2
            0xda, 0x00, // ; mod
            0xac, 0x00, 0x03, 0x00, // 3
            0xdf, 0x00, // Tab()
            0xdd, 0x00, // ;
            0xac, 0x00, 0x04, 0x00, // 4
            0xd9, 0x00, // Print terminator
        ]);
        assert_eq!(
            d.decompile(),
            "Print #1, True, , Tab; ; 1; Spc(1), 2; Tab(3); 4".to_string()
        );
        Ok(())
    }
}

mod decorators {
    use super::*;

    mod dim {
        use super::*;

        const SYMBOLS: &[(u8, char)] = &[
            (2u8, '%'),
            (3, '&'),
            (4, '!'),
            (5, '#'),
            (6, '@'),
            (8, '$'),
            (20, '^'),
        ];

        #[test]
        fn var() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0x5d, 0x00, // Dim
                0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
            ]);
            #[rustfmt::skip]
            let functbl: &mut [u8] = &mut [
                // var def
                0x50, 0x84, // flags
                (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
                0xff, 0xff, 0xff, 0xff, // unk1
                0xff, 0xff, 0xff, 0xff, // unk2
                0xff, 0xff, 0xff, 0xff, // unk3
                0x02, 0x00, 0xff, 0xff, // bltin
                0x00, 0x00, 0x00, 0x00, // unk4
                0x00, 0x00, 0x00, 0x00, // nextvar etc
                0xff, 0xff, 0xff, 0xff, // arg flags
            ];
            for (val, suf) in SYMBOLS {
                functbl[16] = *val;
                d.set_functbl(functbl);
                assert_eq!(d.decompile(), format!("Dim {}{}", NOAS.1, suf));
            }
            Ok(())
        }

        #[test]
        fn full_array() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0x5d, 0x00, // Dim
                0xd1, 0x00, // sep
                0xac, 0x00, 0x0a, 0x00, // 10
                0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
            ]);
            #[rustfmt::skip]
            let functbl: &mut [u8] = &mut[
                // var def
                0x10, 0x84, // flags
                (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
                0xff, 0xff, 0xff, 0xff, // unk1
                0xff, 0xff, 0xff, 0xff, // unk2
                0xff, 0xff, 0xff, 0xff, // unk3
                0x20, 0x00, 0x00, 0x00, // bltin
                0x00, 0x00, 0x00, 0x00, // unk4
                0x00, 0x00, 0x00, 0x00, // nextvar etc
                0xff, 0xff, 0xff, 0xff, // arg flags

                // var data
                0x1b, 0x00, // extra flags
                0x28, 0x00, 0x00, 0x00, // count off
                0x08, 0x00, // type
                0x01, 0x00, // count
            ];
            for (val, suf) in SYMBOLS {
                functbl[0x26] = *val;
                d.set_functbl(functbl);
                assert_eq!(d.decompile(), format!("Dim {}{}(10)", NOAS.1, suf));
            }
            Ok(())
        }

        #[test]
        fn empty_array() -> Result<(), io::Error> {
            let pc = mkpc();
            let mut d = DecTester::new(&pc);

            #[rustfmt::skip]
            d.set_code(&[
                0x5d, 0x00, // Dim
                0xf5, 0x04, 0x00, 0x00, 0x00, 0x00, // Var offset into fn table
            ]);
            #[rustfmt::skip]
            let functbl: &mut [u8] = &mut [
                // var def
                0x10, 0x84, // flags
                (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
                0xff, 0xff, 0xff, 0xff, // unk1
                0xff, 0xff, 0xff, 0xff, // unk2
                0xff, 0xff, 0xff, 0xff, // unk3
                0x20, 0x00, 0x00, 0x00, // bltin
                0x00, 0x00, 0x00, 0x00, // unk4
                0x00, 0x00, 0x00, 0x00, // nextvar etc
                0x00, 0x00, 0x08, 0x00, // arg flags

                // var data
                0x1b, 0x08, // extra flags
                0x28, 0x00, 0x00, 0x00, // count off
                0x08, 0x00, // type
                0x00, 0x00, // count
            ];
            for (val, suf) in SYMBOLS {
                functbl[0x26] = *val;
                d.set_functbl(functbl);
                assert_eq!(d.decompile(), format!("Dim {}{}()", NOAS.1, suf));
            }
            Ok(())
        }
    }

    #[test]
    fn redim() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0xac, 0x00, 0x01, 0x00, // 1
            0xac, 0x00, 0x07, 0x00, // 7
            0xe4, 0x20, // ReDim
            (NOAS.0<<1) as u8, (NOAS.0>>7) as u8, // var<<1
            0x01, 0x00, // cnt
            0x00, 0x00, 0x00, 0x00, // Var offset into fn table
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // var data
            0x1b, 0x08, // extra flags
            0x08, 0x00, 0x00, 0x00, // count off
            0x08, 0x00, // type
            0x01, 0x00, // count - unused
        ]);
        assert_eq!(d.decompile(), format!("ReDim {}$(1 To 7)", NOAS.1));
        Ok(())
    }

    #[test]
    fn args() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x0117, "MySub".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x04, // Sub
            0x00, 0x00, 0x00, 0x00, // offset
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Sub definition
            0x0c, 0x11, // sub flags
            0x2e, 0x02, // sub nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0xff, 0xff, 0xff, 0xff, // ret bltin
            0xff, 0xff, // imp offset
            0xff, 0xff,
            0x07, 0x00, 0x07, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x94, // ret type
            0x01, 0x00, // var count
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var
            0x59, 0x83, // flags
            0x04, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x02, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x80, 0x01, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), format!("Sub MySub(var_bltin%)"));
        Ok(())
    }

    #[test]
    fn decorated_const() -> Result<(), io::Error> {
        let pc = mkpc();
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x5d, 0x04, // Dim
            0xb9, 0x00, 0x03, 0x00, 0x31, 0x32, 0x33, 0x00, // "123"
            0xf5, 0x08, 0x00, 0x00, 0x00, 0x00, // const + offset
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Var
            0x50, 0x94, // flags
            0x04, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x48, 0x00, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0xff, 0xff, 0xff, 0xff, // arg flags
        ]);
        assert_eq!(d.decompile(), format!("Const var_bltin$ = \"123\""));
        Ok(())
    }

    #[test]
    fn function() -> Result<(), io::Error> {
        let mut pc = mkpc();
        pc.string_table.insert(0x11c, "DecoratedFunc".to_string());
        pc.string_table.insert(0x117, "a".to_string());
        let mut d = DecTester::new(&pc);

        #[rustfmt::skip]
        d.set_code(&[
            0x96, 0x08, 0x00, 0x00, 0x00, 0x00, // func definition
        ]);
        #[rustfmt::skip]
        d.set_functbl(&[
            // Function definition
            0x1c, 0x11, // func flags
            0x38, 0x02, // func nameid
            0xff, 0xff, 0xff, 0xff, // next sub
            0xff, 0xff, // ord
            // unks
            0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x58, 0x00, 0x00, 0x00, // first var offset
            0x06, 0x00, 0xff, 0xff, // ret_bltin_or_offset
            0xff, 0xff, 0xff, 0xff,
            0x03, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0xbc, // ret type
            0x02, // var count
            0x00, // vararg
            0x02, // extra vis
            0x00, 0x00, 0x00, 0x00,

            // Var a
            0x59, 0x83, // flags
            0x2e, 0x02, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x08, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0x78, 0x00, 0x00, 0x00, // next offset
            0x80, 0x01, 0x00, 0x00, // arg flags

            // Var Me
            0x69, 0x83, // flags
            0xfe, 0xff, // id
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
            0x06, 0x01, 0xff, 0xff, // bltin
            0x00, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, // next offset
            0x20, 0x00, 0x00, 0x00, // arg flags
        ]);
        assert_eq!(d.decompile(), "Function DecoratedFunc@(a$)");
        Ok(())
    }
}
