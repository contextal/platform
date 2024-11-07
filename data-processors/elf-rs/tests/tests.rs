use elf_rs::ELF;

#[test]
fn parse_elf32() {
    let path = "tests/test_data/test32.elf";
    let input_file =
        std::fs::File::open(&path).unwrap_or_else(|e| panic!("Can't open {path}: {e:#?}"));
    let elf = ELF::new(&input_file).unwrap_or_else(|e| panic!("Can't parse {path}: {e:#?}"));

    // ELF header checks
    let eh = &elf.elf_header;
    assert_eq!(eh.ei_class, "32-bit", "eh.ei_class mismatch");
    assert_eq!(eh.ei_data, "Little-endian", "eh.ei_data mismatch");
    assert_eq!(eh.ei_osabi, "No extensions", "eh.ei_osabi mismatch");
    assert_eq!(eh.e_typestr, "Shared object", "eh.e_typestr mismatch");
    assert_eq!(
        eh.e_machinestr, "Arm (up to Armv7/AArch32)",
        "eh.e_machinestr mismatch"
    );
    assert_eq!(eh.e_entry, 0x400, "eh.e_entry mismatch");
    assert_eq!(eh.e_phoff, 52, "eh.e_phoff mismatch");
    assert_eq!(eh.e_shoff, 4476, "eh.e_shoff mismatch");
    assert_eq!(eh.e_phnum, 9, "eh.e_phnum mismatch");
    assert_eq!(eh.e_shnum, 27, "eh.e_shnum mismatch");
    assert_eq!(eh.e_shentsize, 40, "eh.e_shentsize mismatch");

    // Check program headers
    let p0 = &elf.program_headers[0];
    assert_eq!(p0.p_typestr, "Processor specific", "p0.p_typestr mismatch");
    assert_eq!(p0.p_offset, 0x6e0, "p0.p_offset mismatch");
    assert_eq!(p0.p_vaddr, 0x6e0, "p0.p_vaddr mismatch");
    assert_eq!(p0.p_paddr, 0x6e0, "p0.p_paddr mismatch");
    assert_eq!(p0.p_filesz, 0x8, "p0.p_filesz mismatch");
    assert_eq!(p0.p_memsz, 0x8, "p0.p_memsz mismatch");
    assert_eq!(p0.p_flagsvec, ["READ"], "p0.p_flagsvec mismatch");
    assert_eq!(p0.p_align, 0x4, "p0.p_align mismatch");

    let p4 = &elf.program_headers[4];
    assert_eq!(p4.p_typestr, "Loadable segment", "p4.p_typestr mismatch");
    assert_eq!(p4.p_offset, 0xf08, "p4.p_offset mismatch");
    assert_eq!(p4.p_vaddr, 0x1f08, "p4.p_vaddr mismatch");
    assert_eq!(p4.p_paddr, 0x1f08, "p4.p_paddr mismatch");
    assert_eq!(p4.p_filesz, 0x134, "p4.p_filesz mismatch");
    assert_eq!(p4.p_memsz, 0x138, "p4.p_memsz mismatch");
    assert_eq!(p4.p_flagsvec, ["READ", "WRITE"], "p4.p_flagsvec mismatch");
    assert_eq!(p4.p_align, 0x1000, "p4.p_align mismatch");

    let p8 = &elf.program_headers[8];
    assert_eq!(p8.p_typestr, "OS specific", "p8.p_typestr mismatch");
    assert_eq!(p8.p_offset, 0xf08, "p8.p_offset mismatch");
    assert_eq!(p8.p_vaddr, 0x1f08, "p8.p_vaddr mismatch");
    assert_eq!(p8.p_paddr, 0x1f08, "p8.p_paddr mismatch");
    assert_eq!(p8.p_filesz, 0xf8, "p8.p_filesz mismatch");
    assert_eq!(p8.p_memsz, 0xf8, "p8.p_memsz mismatch");
    assert_eq!(p8.p_flagsvec, ["READ"], "p8.p_flagsvec mismatch");
    assert_eq!(p8.p_align, 0x1, "p8.p_align mismatch");

    // Check section headers
    let s0 = &elf.section_headers[0];
    assert_eq!(s0.sh_namestr, "", "s0.sh_namestr mismatch");
    assert_eq!(s0.sh_typestr, "NULL", "s0.sh_typestr mismatch");
    assert!(s0.sh_flagsvec.is_empty(), "s0.sh_flagsvec mismatch");
    assert_eq!(s0.sh_addr, 0x0, "s0.sh_addr mismatch");
    assert_eq!(s0.sh_offset, 0x0, "s0.sh_offset mismatch");
    assert_eq!(s0.sh_size, 0x0, "s0.sh_size mismatch");
    assert_eq!(s0.sh_addralign, 0x0, "s0.sh_addralign mismatch");

    let s13 = &elf.section_headers[13];
    assert_eq!(s13.sh_namestr, ".text", "s13.sh_namestr mismatch");
    assert_eq!(s13.sh_typestr, "PROGBITS", "s13.sh_typestr mismatch");
    assert_eq!(s13.sh_flagsvec, ["ALLOC", "EXEC"], "s13.sh_flagsvec");
    assert_eq!(s13.sh_addr, 0x400, "s13.sh_addr mismatch");
    assert_eq!(s13.sh_offset, 0x400, "s13.sh_offset mismatch");
    assert_eq!(s13.sh_size, 416, "s13.sh_size mismatch");
    assert_eq!(s13.sh_addralign, 0x4, "s13.sh_addralign mismatch");

    let s26 = &elf.section_headers[26];
    assert_eq!(s26.sh_namestr, ".shstrtab", "s26.sh_namestr mismatch");
    assert_eq!(s26.sh_typestr, "STRTAB", "s26.sh_typestr mismatch");
    assert!(s26.sh_flagsvec.is_empty(), "s26.sh_flagsvec mismatch");
    assert_eq!(s26.sh_addr, 0x0, "s26.sh_addr mismatch");
    assert_eq!(s26.sh_offset, 0x1085, "s26.sh_offset mismatch");
    assert_eq!(s26.sh_size, 245, "s26.sh_size mismatch");
    assert_eq!(s26.sh_addralign, 0x1, "s26.sh_addralign mismatch");
}

#[test]
fn parse_elf64() {
    let path = "tests/test_data/test64.elf";
    let input_file =
        std::fs::File::open(&path).unwrap_or_else(|e| panic!("Can't open {path}: {e:#?}"));
    let elf = ELF::new(&input_file).unwrap_or_else(|e| panic!("Can't parse {path}: {e:#?}"));

    // ELF header checks
    let eh = &elf.elf_header;
    assert_eq!(eh.ei_class, "64-bit", "eh.ei_class mismatch");
    assert_eq!(eh.ei_data, "Little-endian", "eh.ei_data mismatch");
    assert_eq!(eh.ei_osabi, "No extensions", "eh.ei_osabi mismatch");
    assert_eq!(eh.e_typestr, "Shared object", "eh.e_typestr mismatch");
    assert_eq!(eh.e_machinestr, "AMD x86-64", "eh.e_machinestr mismatch");
    assert_eq!(eh.e_entry, 0x1050, "eh.e_entry mismatch");
    assert_eq!(eh.e_phoff, 64, "eh.e_phoff mismatch");
    assert_eq!(eh.e_shoff, 12616, "eh.e_shoff mismatch");
    assert_eq!(eh.e_phnum, 11, "eh.e_phnum mismatch");
    assert_eq!(eh.e_shnum, 28, "eh.e_shnum mismatch");
    assert_eq!(eh.e_shentsize, 64, "eh.e_shentsize mismatch");

    // Check program headers
    let p0 = &elf.program_headers[0];
    assert_eq!(
        p0.p_typestr, "Program header table",
        "p0.p_typestr mismatch"
    );
    assert_eq!(p0.p_offset, 0x40, "p0.p_offset mismatch");
    assert_eq!(p0.p_vaddr, 0x40, "p0.p_vaddr mismatch");
    assert_eq!(p0.p_paddr, 0x40, "p0.p_paddr mismatch");
    assert_eq!(p0.p_filesz, 0x268, "p0.p_filesz mismatch");
    assert_eq!(p0.p_memsz, 0x268, "p0.p_memsz mismatch");
    assert_eq!(p0.p_flagsvec, ["READ"], "p0.p_flagsvec mismatch");
    assert_eq!(p0.p_align, 0x8, "p0.p_align mismatch");

    let p6 = &elf.program_headers[6];
    assert_eq!(
        p6.p_typestr, "Dynamic linking information",
        "p6.p_typestr mismatch"
    );
    assert_eq!(p6.p_offset, 0x2df8, "p6.p_offset mismatch");
    assert_eq!(p6.p_vaddr, 0x3df8, "p6.p_vaddr mismatch");
    assert_eq!(p6.p_paddr, 0x3df8, "p6.p_paddr mismatch");
    assert_eq!(p6.p_filesz, 0x1e0, "p6.p_filesz mismatch");
    assert_eq!(p6.p_memsz, 0x1e0, "p6.p_memsz mismatch");
    assert_eq!(p6.p_flagsvec, ["READ", "WRITE"], "p6.p_flagsvec mismatch");
    assert_eq!(p6.p_align, 0x8, "p6.p_align mismatch");

    let p10 = &elf.program_headers[10];
    assert_eq!(p10.p_typestr, "OS specific", "p10.p_typestr mismatch");
    assert_eq!(p10.p_offset, 0x2de8, "p10.p_offset mismatch");
    assert_eq!(p10.p_vaddr, 0x3de8, "p10.p_vaddr mismatch");
    assert_eq!(p10.p_paddr, 0x3de8, "p10.p_paddr mismatch");
    assert_eq!(p10.p_filesz, 0x218, "p10.p_filesz mismatch");
    assert_eq!(p10.p_memsz, 0x218, "p10.p_memsz mismatch");
    assert_eq!(p10.p_flagsvec, ["READ"], "p10.p_flagsvec mismatch");
    assert_eq!(p10.p_align, 0x1, "p10.p_align mismatch");

    // Check section headers
    let s0 = &elf.section_headers[0];
    assert_eq!(s0.sh_namestr, "", "s0.sh_namestr mismatch");
    assert_eq!(s0.sh_typestr, "NULL", "s0.sh_typestr mismatch");
    assert!(s0.sh_flagsvec.is_empty(), "s0.sh_flagsvec mismatch");
    assert_eq!(s0.sh_addr, 0x0, "s0.sh_addr mismatch");
    assert_eq!(s0.sh_offset, 0x0, "s0.sh_offset mismatch");
    assert_eq!(s0.sh_size, 0x0, "s0.sh_size mismatch");
    assert_eq!(s0.sh_addralign, 0x0, "s0.sh_addralign mismatch");

    let s13 = &elf.section_headers[13];
    assert_eq!(s13.sh_namestr, ".plt.got", "s13.sh_namestr mismatch");
    assert_eq!(s13.sh_typestr, "PROGBITS", "s13.sh_typestr mismatch");
    assert_eq!(s13.sh_flagsvec, ["ALLOC", "EXEC"], "s13.sh_flagsvec");
    assert_eq!(s13.sh_addr, 0x1040, "s13.sh_addr mismatch");
    assert_eq!(s13.sh_offset, 0x1040, "s13.sh_offset mismatch");
    assert_eq!(s13.sh_size, 8, "s13.sh_size mismatch");
    assert_eq!(s13.sh_addralign, 0x8, "s13.sh_addralign mismatch");

    let s27 = &elf.section_headers[27];
    assert_eq!(s27.sh_namestr, ".shstrtab", "s27.sh_namestr mismatch");
    assert_eq!(s27.sh_typestr, "STRTAB", "s27.sh_typestr mismatch");
    assert!(s27.sh_flagsvec.is_empty(), "s27.sh_flagsvec mismatch");
    assert_eq!(s27.sh_addr, 0x0, "s27.sh_addr mismatch");
    assert_eq!(s27.sh_offset, 0x304c, "s27.sh_offset mismatch");
    assert_eq!(s27.sh_size, 247, "s27.sh_size mismatch");
    assert_eq!(s27.sh_addralign, 0x1, "s27.sh_addralign mismatch");
}
