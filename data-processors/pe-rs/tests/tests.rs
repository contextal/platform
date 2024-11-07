use pe_rs::PE;

#[test]
fn parse_pe32() {
    let path = "tests/test_data/test32.exe";
    let input_file =
        std::fs::File::open(&path).unwrap_or_else(|e| panic!("Can't open {path}: {e:#?}"));
    let pe = PE::new(&input_file).unwrap_or_else(|e| panic!("Can't parse {path}: {e:#?}"));

    // PE header checks
    let ph = &pe.pe_header;
    assert_eq!(ph.Machine, 0x14c, "ph.Machine mismatch");
    assert_eq!(ph.NumberOfSections, 8, "ph.NumberOfSections mismatch");
    assert_eq!(
        ph.SizeOfOptionalHeader, 224,
        "ph.SizeOfOptionalHeader mismatch"
    );
    assert_eq!(
        ph.CharacteristicsSymbols,
        [
            "EXECUTABLE_IMAGE",
            "LINE_NUMS_STRIPPED",
            "LOCAL_SYMS_STRIPPED",
            "32BIT_MACHINE",
            "DEBUG_STRIPPED"
        ],
        "ph.CharacteristicsSymbols mismatch"
    );

    // Optional header checks
    let oh = &pe.optional_header;
    assert_eq!(oh.Magic, 0x10b, "oh.Magic mismatch");
    assert_eq!(oh.SizeOfHeaders, 0x400, "oh.SizeOfHeaders mismatch");
    assert_eq!(
        oh.NumberOfRvaAndSizes, 16,
        "oh.NumberOfRvaAndSizes mismatch"
    );

    // Section checks
    let s1 = &pe.section_headers[0];
    assert_eq!(s1.Name, ".text", "s1.Name mismatch");
    assert_eq!(s1.VirtualSize, 0x7044, "s1.VirtualSize mismatch");
    assert_eq!(s1.VirtualAddress, 0x1000, "s1.VirtualAddress mismatch");
    assert_eq!(s1.SizeOfRawData, 29184, "s1.SizeOfRawData mismatch");
    assert_eq!(s1.PointerToRawData, 1024, "s1.PointerToRawData mismatch");
    assert_eq!(
        s1.CharacteristicsSymbols,
        [
            "CNT_CODE",
            "CNT_INITIALIZED_DATA",
            "MEM_EXECUTE",
            "MEM_READ"
        ],
        "s1.CharacteristicsSymbols mismatch"
    );

    let s4 = &pe.section_headers[3];
    assert_eq!(s4.Name, ".bss", "s4.Name mismatch");
    assert_eq!(s4.VirtualSize, 0xa54, "s4.VirtualSize mismatch");
    assert_eq!(s4.VirtualAddress, 0xb000, "s4.VirtualAddress mismatch");
    assert_eq!(s4.SizeOfRawData, 0, "s4.SizeOfRawData mismatch");
    assert_eq!(s4.PointerToRawData, 0, "s4.PointerToRawData mismatch");
    assert_eq!(
        s4.CharacteristicsSymbols,
        ["CNT_UNINITIALIZED_DATA", "MEM_READ", "MEM_WRITE"],
        "s4.CharacteristicsSymbols mismatch"
    );

    let s8 = &pe.section_headers[7];
    assert_eq!(s8.Name, ".reloc", "s8.Name mismatch");
    assert_eq!(s8.VirtualSize, 0x3f4, "s8.VirtualSize mismatch");
    assert_eq!(s8.VirtualAddress, 0xf000, "s8.VirtualAddress mismatch");
    assert_eq!(s8.SizeOfRawData, 1024, "s8.SizeOfRawData mismatch");
    assert_eq!(s8.PointerToRawData, 35840, "s8.PointerToRawData mismatch");
    assert_eq!(
        s8.CharacteristicsSymbols,
        ["CNT_INITIALIZED_DATA", "MEM_DISCARDABLE", "MEM_READ"],
        "s8.CharacteristicsSymbols mismatch"
    );
}

#[test]
fn parse_pe64() {
    let path = "tests/test_data/test64.exe";
    let input_file =
        std::fs::File::open(&path).unwrap_or_else(|e| panic!("Can't open {path}: {e:#?}"));
    let pe = PE::new(&input_file).unwrap_or_else(|e| panic!("Can't parse {path}: {e:#?}"));

    // PE header checks
    let ph = &pe.pe_header;
    assert_eq!(ph.Machine, 0x8664, "ph.Machine mismatch");
    assert_eq!(ph.NumberOfSections, 10, "ph.NumberOfSections mismatch");
    assert_eq!(
        ph.SizeOfOptionalHeader, 240,
        "ph.SizeOfOptionalHeader mismatch"
    );
    assert_eq!(
        ph.CharacteristicsSymbols,
        [
            "EXECUTABLE_IMAGE",
            "LINE_NUMS_STRIPPED",
            "LOCAL_SYMS_STRIPPED",
            "LARGE_ADDRESS_AWARE",
            "DEBUG_STRIPPED"
        ],
        "ph.CharacteristicsSymbols mismatch"
    );

    // Optional header checks
    let oh = &pe.optional_header;
    assert_eq!(oh.Magic, 0x20b, "oh.Magic mismatch");
    assert_eq!(oh.SizeOfHeaders, 0x400, "oh.SizeOfHeaders mismatch");
    assert_eq!(
        oh.NumberOfRvaAndSizes, 16,
        "oh.NumberOfRvaAndSizes mismatch"
    );

    // Section checks
    let s1 = &pe.section_headers[0];
    assert_eq!(s1.Name, ".text", "s1.Name mismatch");
    assert_eq!(s1.VirtualSize, 0x6888, "s1.VirtualSize mismatch");
    assert_eq!(s1.VirtualAddress, 0x1000, "s1.VirtualAddress mismatch");
    assert_eq!(s1.SizeOfRawData, 27136, "s1.SizeOfRawData mismatch");
    assert_eq!(s1.PointerToRawData, 1024, "s1.PointerToRawData mismatch");
    assert_eq!(
        s1.CharacteristicsSymbols,
        [
            "CNT_CODE",
            "CNT_INITIALIZED_DATA",
            "MEM_EXECUTE",
            "MEM_READ"
        ],
        "s1.CharacteristicsSymbols mismatch"
    );

    let s5 = &pe.section_headers[4];
    assert_eq!(s5.Name, ".xdata", "s5.Name mismatch");
    assert_eq!(s5.VirtualSize, 0x43c, "s5.VirtualSize mismatch");
    assert_eq!(s5.VirtualAddress, 0xb000, "s5.VirtualAddress mismatch");
    assert_eq!(s5.SizeOfRawData, 1536, "s5.SizeOfRawData mismatch");
    assert_eq!(s5.PointerToRawData, 32768, "s5.PointerToRawData mismatch");
    assert_eq!(
        s5.CharacteristicsSymbols,
        ["CNT_INITIALIZED_DATA", "MEM_READ"],
        "s5.CharacteristicsSymbols mismatch"
    );

    let s10 = &pe.section_headers[9];
    assert_eq!(s10.Name, ".reloc", "s10.Name mismatch");
    assert_eq!(s10.VirtualSize, 0x84, "s10.VirtualSize mismatch");
    assert_eq!(s10.VirtualAddress, 0x10000, "s10.VirtualAddress mismatch");
    assert_eq!(s10.SizeOfRawData, 512, "s10.SizeOfRawData mismatch");
    assert_eq!(s10.PointerToRawData, 37376, "s10.PointerToRawData mismatch");
    assert_eq!(
        s10.CharacteristicsSymbols,
        ["CNT_INITIALIZED_DATA", "MEM_DISCARDABLE", "MEM_READ"],
        "s10.CharacteristicsSymbols mismatch"
    );
}
