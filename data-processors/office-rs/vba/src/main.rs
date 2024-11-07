use ctxole::Ole;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, Write};
use tracing_subscriber::prelude::*;
use vba::{decomp::CompressContainerReader, forms::*, *};

fn usage(me: &str) -> ! {
    eprintln!("Usage:");
    eprintln!("{} <officefile> [--pc] --project-info", me);
    eprintln!("  Lists all the VBA modules in <officefile>");
    eprintln!("{} <officefile> [--pc] --list-constants", me);
    eprintln!("  Lists all the VBA constants in <officefile>");
    eprintln!("{} <officefile> [--pc] --list-references", me);
    eprintln!("  Lists all the VBA references in <officefile>");
    eprintln!("{} <officefile> [--pc] --list-modules", me);
    eprintln!("  Lists all the VBA modules in <officefile>");
    eprintln!("{} <officefile> [--pc] --module-info <module>", me);
    eprintln!("  Pretty prints the the module <module>");
    eprintln!("{} <officefile> [--pc] --vba <module>", me);
    eprintln!("  Prints the VBA code in <module>");
    eprintln!("{} <officefile> [--pc] --decompile <module>", me);
    eprintln!("  Decompiles the VBA code in <module>");
    eprintln!("--pc: use version-dependent project info (PerformanceCache)");
    eprintln!("{} <officefile> --list-forms", me);
    eprintln!("  Lists all the Forms in <officefile>");
    eprintln!("{} <officefile> --form <form>", me);
    eprintln!("  Pretty prints the the VBA Form <form>");
    eprintln!("{} <olefile> --decompress <stream> <out>", me);
    eprintln!("  Decompresses a CompressedContainer <stream> into <out>");
    std::process::exit(1);
}

macro_rules! print_value {
    ($name: expr, $fmt:expr, $val:expr) => {
        if let Some(v) = $val {
            println!(concat!($name, ": ", $fmt), v);
        } else {
            println!(concat!($name, ": <UNSET>"));
        }
    };
}

fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args.iter().any(|s| s == "--help") {
        usage(&args[0])
    }

    let pc = if args.get(2) == Some(&"--pc".to_string()) {
        args.remove(2);
        true
    } else {
        false
    };
    match args[2].as_ref() {
        "--project-info" | "--list-modules" | "--list-constants" | "--list-references"
        | "--list-forms"
            if args.len() == 3 => {}
        "--module-info" | "--vba" | "--decompile" | "--form" if args.len() == 4 => {}
        "--decompress" if args.len() == 5 => {}
        _ => usage(&args[0]),
    }
    let docf = match File::open(&args[1]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to open {}: {}", args[1], e);
            std::process::exit(1);
        }
    };
    let ole = Ole::new(BufReader::new(docf)).unwrap_or_else(|e| {
        eprintln!("Failed to parse {}: {}", args[1], e);
        std::process::exit(1);
    });

    let macroroot = if ole.get_entry_by_name("Macros").is_ok() {
        "Macros"
    } else if ole.get_entry_by_name("_VBA_PROJECT_CUR").is_ok() {
        "_VBA_PROJECT_CUR"
    } else {
        ""
    };

    if args[2] == "--decompress" {
        decompress(ole, &args[1], &args[3], &args[4])
    } else {
        let vba = Vba::new(&ole, macroroot).unwrap_or_else(|e| {
            eprintln!("Failed to parse VBA data in {}: {}", args[1], e);
            std::process::exit(1);
        });
        let project = if pc {
            vba.project_pc().map(|m| m.as_gen())
        } else {
            vba.project().map(|m| m.as_gen())
        }
        .unwrap_or_else(|e| {
            eprintln!("Error retrieving Project info in {}: {}", args[1], e);
            std::process::exit(1);
        });
        match args[2].as_ref() {
            "--project-info" => {
                print_value!("Name", "{}", project.name());
                print_value!("SysKind", "{}", project.sys_kind());
                if let ProjectGeneric::VI(p) = project {
                    print_value!("Compat version", "{}", p.info.compat_version);
                }
                print_value!("LCID", "{}", project.lcid());
                print_value!("LCID for Invoke", "{}", project.lcid_invoke());
                print_value!("Code page", "{}", project.codepage());
                print_value!("Description", "{}", project.docstring());
                print_value!("Help file", "{}", project.help());
                print_value!("Help context", "{:#x}", project.help_context());
                print_value!(
                    "LIBFLAGS",
                    "{}",
                    project.lib_flags().map(|v| {
                        let mut flags: Vec<&str> = Vec::new();
                        if (v & 1) != 0 {
                            flags.push("RESTRICTED");
                        }
                        if (v & 2) != 0 {
                            flags.push("CONTROL");
                        }
                        if (v & 4) != 0 {
                            flags.push("HIDDEN");
                        }
                        if (v & 8) != 0 {
                            flags.push("HASDISK");
                        }
                        flags.join(" | ")
                    })
                );
                print_value!("Major version", "{:#x}", project.version_major());
                print_value!("Minor version", "{:#x}", project.version_minor());
                print_value!("Cookie", "{:#x}", project.cookie());
                println!("VBA version: {:#x}", vba.vba_project.vba_version);
            }
            "--list-constants" => {
                for c in project.constants() {
                    println!("{} => {}", c.0, c.1);
                }
            }
            "--list-references" => {
                for r in project.references() {
                    let name: &str = optstring2str(&r.name_unicode)
                        .or_else(|| optstring2str(&r.name))
                        .unwrap_or("<UNKNOWN>");
                    print!("- {} ", name);
                    match &r.value {
                        ReferenceValue::Original(_) => {}
                        ReferenceValue::Control(r) => {
                            println!("(Control)");
                            print_value!("   Original", "{}", &r.original.libid_original);
                            print_value!("   Twiddled", "{}", &r.twiddled);
                            print_value!("   Record name (unicode)", "{}", &r.record_name_unicode);
                            print_value!("   Record name (cp)", "{}", &r.record_name);
                            print_value!("   LibID", "{}", &r.libid);
                            println!("   GUID: {}", r.guid);
                            println!("   Cookie: {:#x}", r.cookie);
                        }
                        ReferenceValue::Registered(r) => {
                            println!("(Registered)");
                            print_value!("   LibID", "{}", &r.libid);
                        }
                        ReferenceValue::Project(r) => {
                            println!("(Project)");
                            print_value!("   Absolute", "{}", &r.absolute);
                            print_value!("   Relative", "{}", &r.relative);
                            println!("   Version major: {:#x}", r.version_major);
                            println!("   Version major: {:#x}", r.version_minor);
                        }
                    }
                }
            }
            "--list-modules" => {
                let it = if pc {
                    vba.modules_pc()
                        .unwrap()
                        .map(|m| m.as_gen())
                        .collect::<Vec<ModuleGeneric>>()
                        .into_iter()
                } else {
                    vba.modules()
                        .unwrap()
                        .map(|m| m.as_gen())
                        .collect::<Vec<ModuleGeneric>>()
                        .into_iter()
                };
                for m in it {
                    println!("{}", m.names().first().unwrap_or(&"<NONAME>"));
                }
            }
            "--module-info" | "--vba" | "--decompile" => {
                let mut it = if pc {
                    vba.modules_pc()
                        .unwrap()
                        .map(|m| m.as_gen())
                        .collect::<Vec<ModuleGeneric>>()
                        .into_iter()
                } else {
                    vba.modules()
                        .unwrap()
                        .map(|m| m.as_gen())
                        .collect::<Vec<ModuleGeneric>>()
                        .into_iter()
                };
                let module = it
                    .find(|m| m.names().contains(&args[3].as_str()))
                    .unwrap_or_else(|| {
                        eprintln!("No such module {}", &args[3]);
                        std::process::exit(1);
                    });
                match args[2].as_ref() {
                    "--vba" => match vba.get_code_stream(&module) {
                        Ok(mut stream) => {
                            if let Err(e) = std::io::copy(&mut stream, &mut io::stdout()) {
                                eprintln!(
                                    "Error while decompressing the code stream for module {}: {}",
                                    &args[3], e
                                );
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Error while retrieving the code stream for module {}: {}",
                                &args[3], e
                            );
                            std::process::exit(1);
                        }
                    },
                    "--decompile" => match vba.get_decompiler(&module) {
                        Ok(decompiler) => {
                            for l in decompiler.iter() {
                                match l {
                                    Ok(l) => println!("{}", l),
                                    Err(e) => println!("<Decompiler error: {}>", e),
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error creating decompiler for module {}: {}", &args[3], e);
                            std::process::exit(1);
                        }
                    },
                    "--module-info" => {
                        print_value!("Name", "{}", module.names().first());
                        print_value!("Stream name", "{}", module.stream_names().first());
                        print_value!("Description", "{}", module.docstrings().first());
                        print_value!("Stream offset", "{:#x}", module.offset());
                        print_value!("Help context", "{:#x}", module.help_context());
                        print_value!("Cookie", "{:#x}", module.cookie());
                        println!(
                            "Procedural: {}",
                            if module.is_procedural() { 'Y' } else { 'N' }
                        );
                        println!(
                            "Non-procedural: {}",
                            if module.is_non_procedural() { 'Y' } else { 'N' }
                        );
                        println!(
                            "Read only: {}",
                            if module.is_read_only() { 'Y' } else { 'N' }
                        );
                        println!("Private: {}", if module.is_private() { 'Y' } else { 'N' });
                    }
                    _ => unreachable!(),
                }
            }
            "--list-forms" => {
                for f in vba.forms() {
                    println!("{}", f.0);
                }
            }
            "--form" => {
                let form = vba
                    .forms()
                    .find_map(|f| if f.0 == args[3] { Some(f.1) } else { None })
                    .unwrap_or_else(|| {
                        eprintln!("No such form {}", &args[3]);
                        std::process::exit(1);
                    })
                    .unwrap_or_else(|e| {
                        eprintln!("Error dumping form {}: {}", &args[3], e);
                        std::process::exit(1);
                    });
                println!("UserForm {:?}", &form);
                print_children(form.children());
            }
            _ => {}
        }
    }
}

fn print_children<R: Read + Seek>(it: ChildIterator<R>) {
    for control in it {
        match control {
            Ok(c) => match c {
                Control::Frame(c) => {
                    println!("FRAME {:?}", c);
                    print_children(c.children());
                }
                Control::MultiPage(c) => {
                    println!("MultiPage {:?}", c);
                    print_children(c.children());
                }
                Control::Page(c) => {
                    println!("Page {:?}", c);
                    print_children(c.children());
                }
                Control::Image(c) => println!("IMAGE {:?}", c),
                Control::SpinButton(c) => println!("SPIN {:?}", c),
                Control::CommandButton(c) => println!("BUTTON {:?}", c),
                Control::TabStrip(c) => println!("TAB {:?}", c),
                Control::Label(c) => println!("LABEL {:?}", c),
                Control::TextBox(c) => println!("EDIT {:?}", c),
                Control::ListBox(c) => println!("LIST {:?}", c),
                Control::ComboBox(c) => println!("COMBO {:?}", c),
                Control::CheckBox(c) => println!("CHECK {:?}", c),
                Control::OptionButton(c) => println!("OPTION {:?}", c),
                Control::ToggleButton(c) => println!("TOGGLE {:?}", c),
                Control::ScrollBar(c) => println!("SCROLL {:?}", c),
                Control::UnknownType(t) => println!("Unknown control ({})", t),
            },
            Err(e) => println!("An error occurred: {}", e),
        }
    }
}

fn decompress<R: Read + Seek>(ole: Ole<R>, fname: &str, stream: &str, out: &str) {
    let entry = ole.get_entry_by_name(stream).unwrap_or_else(|e| {
        eprintln!("Error finding stream {} in {}: {}", stream, fname, e);
        std::process::exit(1);
    });
    let mut reader = CompressContainerReader::new(ole.get_stream_reader(&entry), entry.size)
        .unwrap_or_else(|e| {
            eprintln!("Failed to create decompressor: {}", e);
            std::process::exit(1);
        });
    let mut writer: Box<dyn Write> = match out {
        "-" => Box::new(io::stdout()),
        _ => Box::new(File::create(out).unwrap_or_else(|e| {
            eprintln!("Failed to create output file {}: {}", out, e);
            std::process::exit(1);
        })),
    };
    std::io::copy(&mut reader, &mut writer).unwrap_or_else(|e| {
        eprintln!("Copy error: {}", e);
        std::process::exit(1);
    });
}

fn optstring2str(opt: &Option<String>) -> Option<&str> {
    opt.as_deref()
}
