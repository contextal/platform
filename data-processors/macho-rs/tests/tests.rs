use macho_rs::MachO;

#[test]
fn parse_macho_arm64() {
    let path = "tests/test_data/test.arm64";
    let input_file =
        std::fs::File::open(&path).unwrap_or_else(|e| panic!("Can't open {path}: {e:#?}"));
    let macho = MachO::new(&input_file).unwrap_or_else(|e| panic!("Can't parse {path}: {e:#?}"));

    // Mach-O header checks
    let mh = &macho.macho_header;
    assert_eq!(mh.cputypestr, "ARM64", "mh.cputypestr mismatch");
    assert_eq!(mh.filetypestr, "EXECUTE", "mh.filetypestr mismatch");
    assert_eq!(mh.ncmds, 17, "mh.ncmds mismatch");
    assert_eq!(mh.sizeofcmds, 1056, "mh.sizeofcmds mismatch");
    assert_eq!(
        mh.flagsvec,
        ["NOUNDEFS", "DYLDLINK", "TWOLEVEL", "PIE"],
        "mh.flagsvec mismatch"
    );

    // Load command checks
    let lc0 = &macho.load_cmds[0];
    assert_eq!(lc0.cmdstr, "SEGMENT_64", "lc0.cmdstr mismatch");
    assert_eq!(lc0.cmdsize, 72, "lc0.cmdsize mismatch");

    let lc4 = &macho.load_cmds[4];
    assert_eq!(lc4.cmdstr, "DYLD_CHAINED_FIXUPS", "lc4.cmdstr mismatch");
    assert_eq!(lc4.cmdsize, 16, "lc4.cmdsize mismatch");

    let lc16 = &macho.load_cmds[16];
    assert_eq!(lc16.cmdstr, "CODE_SIGNATURE", "lc16.cmdstr mismatch");
    assert_eq!(lc16.cmdsize, 16, "lc16.cmdsize mismatch");

    // Segment checks
    let seg0 = &macho.segment_cmds[0];
    assert_eq!(seg0.segname, "__PAGEZERO", "seg0.segname mismatch");
    assert_eq!(seg0.vmaddr, 0x0, "seg0.vmaddr mismatch");
    assert_eq!(seg0.vmsize, 0x100000000, "seg0.vmsize mismatch");
    assert_eq!(seg0.fileoff, 0, "seg0.fileoff mismatch");
    assert_eq!(seg0.filesize, 0, "seg0.filesize mismatch");
    assert_eq!(seg0.nsects, 0, "seg0.nsects mismatch");

    let seg3 = &macho.segment_cmds[3];
    assert_eq!(seg3.segname, "__LINKEDIT", "seg3.segname mismatch");
    assert_eq!(seg3.vmaddr, 0x100008000, "seg3.vmaddr mismatch");
    assert_eq!(seg3.vmsize, 0x4000, "seg3.vmsize mismatch");
    assert_eq!(seg3.fileoff, 32768, "seg3.fileoff mismatch");
    assert_eq!(seg3.filesize, 680, "seg3.filesize mismatch");
    assert_eq!(seg3.nsects, 0, "seg3.nsects mismatch");

    // Section check
    let sec0 = &macho.sections[0];
    assert_eq!(sec0.sectname, "__text", "sec0.sectname mismatch");
    assert_eq!(sec0.segname, "__TEXT", "sec0.segname mismatch");
    assert_eq!(sec0.addr, 0x100003f4c, "sec0.addr mismatch");
    assert_eq!(sec0.size, 60, "sec0.size mismatch");
    assert_eq!(sec0.offset, 16204, "sec0.offset mismatch");

    let sec4 = &macho.sections[4];
    assert_eq!(sec4.sectname, "__got", "sec4.sectname mismatch");
    assert_eq!(sec4.segname, "__DATA_CONST", "sec4.segname mismatch");
    assert_eq!(sec4.addr, 0x100004000, "sec4.addr mismatch");
    assert_eq!(sec4.size, 8, "sec4.size mismatch");
    assert_eq!(sec4.offset, 16384, "sec4.offset mismatch");
}

#[test]
fn parse_macho_x86_64() {
    let path = "tests/test_data/test.x86_64";
    let input_file =
        std::fs::File::open(&path).unwrap_or_else(|e| panic!("Can't open {path}: {e:#?}"));
    let macho = MachO::new(&input_file).unwrap_or_else(|e| panic!("Can't parse {path}: {e:#?}"));

    // Mach-O header checks
    let mh = &macho.macho_header;
    assert_eq!(mh.cputypestr, "X86_64", "mh.cputypestr mismatch");
    assert_eq!(mh.filetypestr, "EXECUTE", "mh.filetypestr mismatch");
    assert_eq!(mh.ncmds, 16, "mh.ncmds mismatch");
    assert_eq!(mh.sizeofcmds, 1040, "mh.sizeofcmds mismatch");
    assert_eq!(
        mh.flagsvec,
        ["NOUNDEFS", "DYLDLINK", "TWOLEVEL", "PIE"],
        "mh.flagsvec mismatch"
    );

    // Load command checks
    let lc0 = &macho.load_cmds[0];
    assert_eq!(lc0.cmdstr, "SEGMENT_64", "lc0.cmdstr mismatch");
    assert_eq!(lc0.cmdsize, 72, "lc0.cmdsize mismatch");

    let lc4 = &macho.load_cmds[4];
    assert_eq!(lc4.cmdstr, "DYLD_CHAINED_FIXUPS", "lc4.cmdstr mismatch");
    assert_eq!(lc4.cmdsize, 16, "lc4.cmdsize mismatch");

    let lc16 = &macho.load_cmds[15];
    assert_eq!(lc16.cmdstr, "DATA_IN_CODE", "lc16.cmdstr mismatch");
    assert_eq!(lc16.cmdsize, 16, "lc16.cmdsize mismatch");

    // Segment checks
    let seg0 = &macho.segment_cmds[0];
    assert_eq!(seg0.segname, "__PAGEZERO", "seg0.segname mismatch");
    assert_eq!(seg0.vmaddr, 0x0, "seg0.vmaddr mismatch");
    assert_eq!(seg0.vmsize, 0x100000000, "seg0.vmsize mismatch");
    assert_eq!(seg0.fileoff, 0, "seg0.fileoff mismatch");
    assert_eq!(seg0.filesize, 0, "seg0.filesize mismatch");
    assert_eq!(seg0.nsects, 0, "seg0.nsects mismatch");

    let seg3 = &macho.segment_cmds[3];
    assert_eq!(seg3.segname, "__LINKEDIT", "seg3.segname mismatch");
    assert_eq!(seg3.vmaddr, 0x100008000, "seg3.vmaddr mismatch");
    assert_eq!(seg3.vmsize, 0x100, "seg3.vmsize mismatch");
    assert_eq!(seg3.fileoff, 32768, "seg3.fileoff mismatch");
    assert_eq!(seg3.filesize, 256, "seg3.filesize mismatch");
    assert_eq!(seg3.nsects, 0, "seg3.nsects mismatch");

    // Section check
    let sec0 = &macho.sections[0];
    assert_eq!(sec0.sectname, "__text", "sec0.sectname mismatch");
    assert_eq!(sec0.segname, "__TEXT", "sec0.segname mismatch");
    assert_eq!(sec0.addr, 0x100003f70, "sec0.addr mismatch");
    assert_eq!(sec0.size, 44, "sec0.size mismatch");
    assert_eq!(sec0.offset, 16240, "sec0.offset mismatch");

    let sec4 = &macho.sections[4];
    assert_eq!(sec4.sectname, "__got", "sec4.sectname mismatch");
    assert_eq!(sec4.segname, "__DATA_CONST", "sec4.segname mismatch");
    assert_eq!(sec4.addr, 0x100004000, "sec4.addr mismatch");
    assert_eq!(sec4.size, 8, "sec4.size mismatch");
    assert_eq!(sec4.offset, 16384, "sec4.offset mismatch");
}
