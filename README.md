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
    -d, --dot-files-excluded       ignore files and directories where the basename starts with a dot. This is equivalent
                                   to -i '^[.].*'
    -e, --empty-dirs-ignored       if enabled, empty directories containing no or only ignored files are excluded. The
                                   default is to include them
    -h, --help                     Prints help information
    -s, --symlinks-should-abort    program should stop if it encounters an symlink. The default behaviour is to replace
                                   all symlinks with the "actual" content of the files/dirs behind the symlinks. Please
                                   note that this program will never put actual symlinks into the tar file, it will
                                   always duplicate the content of the actual file where the symlink points to!
    -V, --version                  Prints version information

OPTIONS:
    -i, --ignored-names <ignored-names>...    list of regular expressions. If the regular expression matches the file or
                                              directory basename, then this file or directory (including potential
                                              subdirectories and files) will not be included into the archive
    -m, --main-dir-name <main-dir-name>       (optional) name if you want to rename base directory or (in case of
                                              single-file tar) the main file
        --output-hash <output-hash>           optionally, you can get the list of SHA512 hashes of included files. It
                                              will be written to the filename or you can use "-" for stdout
    -o, --output-tar <output-tar>             where to write the tar output to, use "-" for stdout [default: -]

ARGS:
    <input>    Input directory (or single file)
```

