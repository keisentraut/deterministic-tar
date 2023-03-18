// use hex::encode;
use regex::Regex;
use sha2::{Digest, Sha512};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use structopt::StructOpt;

fn parse_regex(src: &str) -> Result<Regex, regex::Error> {
    Ok(Regex::new(src)?)
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(
    name = "deterministic-tar",
    about = "Create a byte-deterministic tar archive of directories, just based on filename and content, nothing else."
)]
struct DeterministicTarOpt {
    /// Input directory (or single file)
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// where to write the tar output to, use "-" for stdout
    #[structopt(short, long, default_value = "-")]
    output_tar: String,

    /// optionally, you can get the list of SHA512 hashes of included files. It will be written to the filename or you can use "-" for stdout.
    #[structopt(long)]
    output_hash: Option<String>,

    /// (optional) name if you want to rename base directory or (in case of single-file tar) the main file
    #[structopt(short, long)]
    main_dir_name: Option<String>,

    /// list of regular expressions. If the regular expression matches the file or directory basename, then this file or directory (including potential subdirectories and files) will not be included into the archive.
    #[structopt(short, long, parse(try_from_str = parse_regex))]
    ignored_names: Vec<Regex>,

    /// if enabled, empty directories containing no or only ignored files are excluded. The default is to include them.
    #[structopt(short, long)]
    empty_dirs_ignored: bool,

    /// program should stop if it encounters an symlink. The default behaviour is to replace all symlinks with the "actual" content of the files/dirs behind the symlinks. Please note that this program will never put actual symlinks into the tar file, it will always duplicate the content of the actual file where the symlink points to!
    #[structopt(short, long)]
    symlinks_should_abort: bool,

    /// ignore files and directories where the basename starts with a dot. This is equivalent to -i '^[.].*'
    #[structopt(short, long)]
    dot_files_excluded: bool,
}

#[derive(Clone, Debug)]
enum DirWalkType {
    Directory,
    File,
    SymlinkToFile(PathBuf),
    SymlinkToDirectory(PathBuf),
}

#[derive(Clone, Debug)]
struct DirWalkItem {
    abspath: PathBuf,
    relpath: PathBuf,
    typ: DirWalkType,
    size: Option<u64>,
}

#[derive(Clone, Debug)]
struct DirWalkIterator {
    empty_dirs_ignored: bool,
    symlinks_should_abort: bool,
    ignored_filenames: Vec<Regex>,
    remaining: Vec<PathBuf>,
    basedir: PathBuf,
}

impl DirWalkIterator {
    fn new(
        basedir: &PathBuf,
        remaining: &Vec<PathBuf>,
        ignored_filenames: &Vec<Regex>,
        empty_dirs_ignored: &bool,
        symlinks_should_abort: &bool,
    ) -> DirWalkIterator {
        DirWalkIterator {
            empty_dirs_ignored: empty_dirs_ignored.clone(),
            symlinks_should_abort: symlinks_should_abort.clone(),
            ignored_filenames: ignored_filenames.clone(),
            remaining: remaining.clone(),
            basedir: basedir.clone(),
        }
    }
}

fn is_allowed_name(p: &PathBuf, i: &Vec<Regex>) -> bool {
    let p = p
        .file_name()
        .unwrap()
        .to_str()
        .expect(format!("cannot convert PathBuf {:?} to string", &p).as_str());
    // now check if we match any "ignored_filenames regex"
    !i.iter().any(|regex| regex.is_match(p))
}

impl Iterator for DirWalkIterator {
    type Item = DirWalkItem;
    fn next(&mut self) -> Option<DirWalkItem> {
        if let Some(r) = self.remaining.pop() {
            let sym_meta =
                std::fs::symlink_metadata(&r).expect(format!("stat for {:?} failed", &r).as_str());
            let abspath = r.clone();
            let relpath = r
                .clone()
                .strip_prefix(&self.basedir)
                .expect("could not strip prefix")
                .to_path_buf();
            //dbg!(&relpath, &abspath);
            if sym_meta.is_symlink() {
                if self.symlinks_should_abort {
                    panic!("Found symlink at {:?}, aborting.", &abspath);
                };
                let resolved_path = r
                    .canonicalize()
                    .expect(format!("error resolving symlink {:?}", &r).as_str());
                let resolved_meta = std::fs::symlink_metadata(&resolved_path)
                    .expect(format!("stat for {:?} failed", &resolved_path).as_str());
                if resolved_meta.is_dir() {
                    return Some(DirWalkItem {
                        relpath: relpath,
                        abspath: abspath,
                        typ: DirWalkType::SymlinkToDirectory(resolved_path),
                        size: Some(resolved_meta.size()),
                    });
                } else if resolved_meta.is_file() {
                    return Some(DirWalkItem {
                        relpath: relpath,
                        abspath: abspath,
                        typ: DirWalkType::SymlinkToFile(resolved_path),
                        size: Some(resolved_meta.size()),
                    });
                } else {
                    unreachable!("");
                }
            }
            if sym_meta.is_file() {
                return Some(DirWalkItem {
                    relpath: relpath,
                    abspath: abspath,
                    typ: DirWalkType::File,
                    size: Some(sym_meta.size()),
                });
            }
            if sym_meta.is_dir() {
                let mut subs: Vec<PathBuf> = r
                    .read_dir()
                    .expect(format!("can't read directory {:?}", &r).as_str())
                    .map(|i| i.expect("intermittent i/o error").path())
                    .filter(|d| {
                        is_allowed_name(
                            &d.strip_prefix(&self.basedir)
                                .expect("could not strip prefix")
                                .to_path_buf(),
                            &self.ignored_filenames,
                        )
                    })
                    .collect();
                // if the directory is empty and we shouldn't include empty directories, then we proceed with empty dir
                if subs.is_empty() && self.empty_dirs_ignored {
                    return self.next();
                }
                // sort in reverse alphabetically order
                subs.sort_by(|a, b| b.cmp(a));
                self.remaining.append(&mut subs);
                return Some(DirWalkItem {
                    relpath: relpath,
                    abspath: abspath,
                    typ: DirWalkType::Directory,
                    size: None,
                });
            }
            unreachable!("Neither symlink, file nor dir!");
        } else {
            // nothing left
            None
        }
    }
}

struct TarOutput {}
impl TarOutput {
    fn _tar_fix_header_checksum(header: &mut Vec<u8>) {
        let mut sum = 0u64;
        drop(
            header
                .iter()
                .map(|i| {
                    sum += *i as u64;
                })
                .collect::<Vec<_>>(),
        );
        // checksum is now correct
        header[148..156].clone_from_slice(format!("{:06o}\x00 ", sum).as_bytes());
    }

    fn tar_write_dir(out_tar: &mut impl Write, tarname: &[u8]) -> Result<(), std::io::Error> {
        if tarname.len() > 100 {
            // first create a longlink
            let mut header: Vec<u8> = vec![0u8; 512];
            header[0..13].clone_from_slice(b"././@LongLink");
            header[100..108].clone_from_slice(b"0000755\x00"); // File mode (octal)
            header[108..116].clone_from_slice(b"0000000\x00"); // Owner's numeric user ID (octal), here we use 0 for "root"
            header[116..124].clone_from_slice(b"0000000\x00"); // Group's numeric user ID (octal), here we use 0 for "root"
            header[124..136].clone_from_slice(format!("{:011o}\x00", tarname.len()).as_bytes()); // longlink name length bytes (octal), zero for a directory
            header[148..156].clone_from_slice(b"        "); // checksum: eight spaces, will be replaced later
            header[156] = b'L'; // magic value for "LongLink"
            header[257..265].clone_from_slice(b"ustar  \x00"); // magic string for ustar format extension, version 00
            header[265..269].clone_from_slice(b"root"); // Owner user name
            header[297..301].clone_from_slice(b"root"); // Owner group name
            TarOutput::_tar_fix_header_checksum(&mut header);
            out_tar.write_all(&header)?;

            // now, write LongLink entry padded to 512 bytes
            let padding = ((512 - (tarname.len() % 512)) % 512) as usize;
            out_tar.write_all(tarname)?;
            out_tar.write_all(&[0u8; 512][..padding])?;
        }

        let mut header: Vec<u8> = vec![0u8; 512];
        header[0..std::cmp::min(tarname.len(), 100)]
            .clone_from_slice(&tarname[..std::cmp::min(tarname.len(), 100)]);
        header[100..108].clone_from_slice(b"0000755\x00"); // File mode (octal)
        header[108..116].clone_from_slice(b"0000000\x00"); // Owner's numeric user ID (octal), here we use 0 for "root"
        header[116..124].clone_from_slice(b"0000000\x00"); // Group's numeric user ID (octal), here we use 0 for "root"
        header[124..136].clone_from_slice(b"00000000000\x00"); // File size in bytes (octal), zero for a directory
        header[148..156].clone_from_slice(b"        "); // checksum: eight spaces, will be replaced later
        header[156] = b'5';
        header[257..265].clone_from_slice(b"ustar  \x00"); // magic string for ustar format extension, version 00
        header[265..269].clone_from_slice(b"root"); // Owner user name
        header[297..301].clone_from_slice(b"root"); // Owner group name
        TarOutput::_tar_fix_header_checksum(&mut header);
        out_tar.write_all(&header)
    }

    fn tar_write_file(
        out_tar: &mut impl Write,
        out_hash: Option<&mut impl Write>,
        in_filedescriptor: &mut BufReader<File>,
        size: &u64,
        tarname: &[u8],
    ) -> Result<(), std::io::Error> {
        if tarname.len() > 100 {
            // first create a longlink
            let mut header: Vec<u8> = vec![0u8; 512];
            header[0..13].clone_from_slice(b"././@LongLink");
            header[100..108].clone_from_slice(b"0000644\x00"); // File mode (octal)
            header[108..116].clone_from_slice(b"0000000\x00"); // Owner's numeric user ID (octal), here we use 0 for "root"
            header[116..124].clone_from_slice(b"0000000\x00"); // Group's numeric user ID (octal), here we use 0 for "root"
            header[124..136].clone_from_slice(format!("{:011o}\x00", tarname.len()).as_bytes()); // longlink name length bytes (octal), zero for a directory
            header[148..156].clone_from_slice(b"        "); // checksum: eight spaces, will be replaced later
            header[156] = b'L'; // magic value for "LongLink"
            header[257..265].clone_from_slice(b"ustar  \x00"); // magic string for ustar format extension, version 00
            header[265..269].clone_from_slice(b"root"); // Owner user name
            header[297..301].clone_from_slice(b"root"); // Owner group name
            TarOutput::_tar_fix_header_checksum(&mut header);
            out_tar.write_all(&header)?;

            // now, write LongLink padded to 512 bytes
            out_tar.write_all(tarname)?;
            let padding = if tarname.len() % 512 == 0 {
                0
            } else {
                512 - (tarname.len() % 512)
            };
            out_tar.write_all(&[0u8; 512][..padding])?;
        }
        let mut header: Vec<u8> = vec![0u8; 512];
        header[0..std::cmp::min(tarname.len(), 100)]
            .clone_from_slice(&tarname[..std::cmp::min(tarname.len(), 100)]);
        header[100..108].clone_from_slice(b"0000644\x00"); // File mode (octal)
        header[108..116].clone_from_slice(b"0000000\x00"); // Owner's numeric user ID (octal), here we use 0 for "root"
        header[116..124].clone_from_slice(b"0000000\x00"); // Group's numeric user ID (octal), here we use 0 for "root"
        header[124..136].clone_from_slice(format!("{:011o}\x00", size).as_bytes()); // File size in bytes (octal), zero for a directory
        header[148..156].clone_from_slice(b"        "); // checksum: eight spaces, will be replaced later
        header[156] = b'0'; // magic value for "normal file"
        header[257..265].clone_from_slice(b"ustar  \x00"); // magic string for ustar format extension, version 00
        header[265..269].clone_from_slice(b"root"); // Owner user name
        header[297..301].clone_from_slice(b"root"); // Owner group name
        TarOutput::_tar_fix_header_checksum(&mut header);

        out_tar.write_all(&header)?;

        // // now we have to write the file in 512 bytes block and pad it with zero bytes on end
        let mut already_read = 0u64;
        let mut buffer = [0; 512];
        let mut sha512_hasher = Sha512::new();
        loop {
            let n = in_filedescriptor.read(&mut buffer)?;
            if n == 0 {
                break;
            };
            already_read += n as u64;
            out_tar
                .write_all(&buffer[0..n])
                .expect("could not write to tarfile");
            if out_hash.is_some() {
                sha512_hasher.update(&buffer[0..n]);
            };
        }
        if already_read != *size {
            panic!("size while reading different from stat");
        }
        let padding = ((512 - (already_read % 512)) % 512) as usize;
        out_tar.write_all(&[0u8; 512][..padding])?;
        if out_hash.is_some() {
            let digest = sha512_hasher.finalize();
            let out_hash = out_hash.unwrap();
            out_hash.write_all(hex::encode(&digest).as_bytes())?;
            out_hash.write_all(b"  ")?;
            out_hash.write_all(tarname)?;
            out_hash.write_all(b"\n")?;
        }
        Ok(())
    }

    fn tar_end_marker(out_tar: &mut impl Write) -> Result<(), std::io::Error> {
        // tar archives ends with 2 blocks of zeros, each 512 bytes
        // actually, gnu tar creates 10 empty blocks but 2 blocks are strictly spoken already sufficient
        out_tar.write_all(&[0u8; 10 * 512])
    }
}

fn validate_main_dir_name(m: &Option<String>) -> Option<PathBuf> {
    match m {
        Some(s) => {
            if s.starts_with("/") || s.ends_with("/") {
                panic!("main dir name must not start or end with /");
            } else {
                let mut p = PathBuf::new();
                p.push(s.clone());
                Some(p)
            }
        }
        None => None,
    }
}

fn main() {
    // command line argument parsing
    let opt = DeterministicTarOpt::from_args();

    let mut ignored_names = opt.ignored_names.clone();
    if opt.dot_files_excluded {
        ignored_names.push(Regex::new(r"^[.].*$").unwrap());
    }
    let input = opt
        .input
        .canonicalize()
        .expect("error getting absolute path of input file/directory");

    // prepare output streams
    let mut stdout_used: usize = 0;
    let mut output_tar: Box<dyn Write> = if opt.output_tar == String::from("-") {
        stdout_used += 1;
        Box::new(std::io::stdout())
    } else {
        Box::new(
            std::fs::File::create(&opt.output_tar)
                .expect(format!("could not open file {:?}", &opt.output_tar).as_str()),
        )
    };
    let mut output_hash: Option<Box<dyn Write>> = if opt.output_hash == Some(String::from("-")) {
        stdout_used += 1;
        Some(Box::new(std::io::stdout()))
    } else {
        if opt.output_hash == None {
            None
        } else {
            let filename = opt.output_hash.unwrap();
            Some(Box::new(std::fs::File::create(&filename).expect(
                format!("could not open file {:?}", &filename).as_str(),
            )))
        }
    };
    if stdout_used > 1 {
        panic!("Stdout used for more than one argument!");
    }

    let parent = input
        .parent()
        .expect("input directory has no parent!")
        .to_path_buf();
    let main_dir_name =
        validate_main_dir_name(&opt.main_dir_name).unwrap_or(input.file_name().unwrap().into());
    let remaining = vec![input.clone()];

    // now, iterate through all files
    for d in DirWalkIterator::new(
        &parent,
        &remaining,
        &ignored_names,
        &opt.empty_dirs_ignored,
        &opt.symlinks_should_abort,
    ) {
        let mut tarname = main_dir_name.clone();
        for p in d.relpath.iter().skip(1) {
            tarname.push(p);
        }
        match d.typ {
            DirWalkType::Directory | DirWalkType::SymlinkToDirectory(_) => {
                // create trailing slash at end
                tarname.push("");
                TarOutput::tar_write_dir(&mut output_tar, tarname.to_str().unwrap().as_bytes())
            }
            DirWalkType::File => TarOutput::tar_write_file(
                &mut output_tar,
                output_hash.as_mut(),
                &mut BufReader::new(std::fs::File::open(&d.abspath).unwrap()),
                &d.size.unwrap(),
                tarname.to_str().unwrap().as_bytes(),
            ),
            DirWalkType::SymlinkToFile(resolved_path) => TarOutput::tar_write_file(
                &mut output_tar,
                output_hash.as_mut(),
                &mut BufReader::new(std::fs::File::open(resolved_path).unwrap()),
                &d.size.unwrap(),
                tarname.to_str().unwrap().as_bytes(),
            ),
        }
        .unwrap();
    }
    TarOutput::tar_end_marker(&mut output_tar).unwrap();
}
