# A backend for processing RAR archives #
The backend uses unrar C++ library by rarlab. The library supports RAR archives
produced by `WinRAR` up to and including version `WinRAR` version 6.

## The backend provides the following information about the archive itself: ##
- archive comment
- list of directories
- whether archive uses encrypted headers
- whether archive has a recovery record
- whether archive is locked/has lock attribute
- whether archive file is the first volume in multivolume archive
- whether archive is one of volumes in multivolume archive
- whether archive follows new numbering scheme for volume names
- whether archive is signed
- whether archive is solid

## Also backend provides the following information about each archive entry of file type: ##
- file modification time
- optional `atime`, `mtime` (different precision and format from the above) and `ctime`
- operating system which created an archive entry
- file attributes (according to operating system which created an archive entry)
- hash function used to verify entry integrity
- `CRC32` hash
- optional `BLAKE2sp` hash
- whether the entry is encrypted
- file path-and-name
- dictionary size
- compressed size, optional uncompressed size and optional compress ratio
- unrar version required to extract an entry
- compression method
- redirection type (none, symlink, hardlink, junction, etc)
- optional redirection target

# UnRAR library API #
For UnRAR API documentation see internals of "UnRAR dynamic library for Windows
software developers" archive on `https://www.rarlab.com/rar_add.htm`

# UnRAR source code #
`vendor/unrar` directory contains unmodified UnRAR source code.
The code may not be used to develop a RAR (WinRAR) compatible archiver.
See `vendor/unrar/readme.txt` and `vendor/unrar/license.txt` for more details.
