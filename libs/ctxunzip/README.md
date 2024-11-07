# Unzip library #

Written from scratch, based on APPNOTE 6.3.10

# Design goals and implementation #

The main development goal, besides data decompression, is to provide
extensive and transparent access to the numerous zip structures and
the metadata held within them

The code is a mixture of native implementations, native crates and
FFI-wrapped external libraries (possibly imported through a crate).
The native implementation is always preferred unless a native crate
exists which is proven and reputable. A native implementation (code
or crate) is always preferred unless a FFI library exists which is
proven, reputable and performs at least 2x better.

# Supported Zip features #
Supported methods:
- Store
- Shrink
- Reduce
- Implode
- Deflate
- Enhanced deflate (Deflate64)
- Bzip2
- LZMA (LZMA1)
- Zstandard
- XZ (LZMA2)

Supported encryption types:
- Traditional PKWARE encryption
- WinZip AE-1 and AE-2 (AES128, AES192, AES256)
