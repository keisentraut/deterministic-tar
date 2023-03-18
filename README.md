# deterministic-tar

This is a Linux CLI program written in Rust which creates byte-identical tar files only based on

* directory structure and file names
* file contents

It does not include file modification timestamps nor file ownership or 
Additionally, it takes care that all files are always ordered alphabetically.

It supports file names >100 chars but does not support symlinks.
All symlinks will be replaced with the content of the final file they are pointing too.
Optional, you can make it abort if it encounters a symlink.


# Compiling

```
$ cargo build --release
```

# Usage

```
$ target/release/deterministic-tar --help
deterministic-tar 0.1.0
Create a byte-deterministic tar archive of directories, just based on filename and content, nothing else.

USAGE:
    deterministic-tar [FLAGS] [OPTIONS] <input>

FLAGS:
    -d, --dot-files-excluded       ignore files starting with ".", this is equivalent to -i '^[.].*'
    -e, --empty-dirs-ignored       if enabled, empty directories containing no or only ignored files are excluded. The
                                   default is to include them
    -h, --help                     Prints help information
    -s, --symlinks-should-abort    program should stop if it encounters an symlink. The default behaviour is to replace
                                   all symlinks with the "actual" content of the files/dirs behind the symlinks. Please
                                   note that this program will never put actual symlinks into the directories, it will
                                   always replace it with the actual file!
    -V, --version                  Prints version information

OPTIONS:
    -i, --ignored-names <ignored-names>...    list of regular expressions. If the regular expression matches the file or
                                              directory name (without subdirs), then the file or directory (including
                                              directory) will not be included into the archive
    -m, --main-dir-name <main-dir-name>       (optional) directory name with which it will be required
        --output-hash <output-hash>           SHA512 hashes of included files will be written to this file, use "-" for
                                              stdout
    -o, --output-tar <output-tar>             where to write the tar output to, use "-" for stdout [default: -]

ARGS:
    <input>    Input directory (or single file)
```

