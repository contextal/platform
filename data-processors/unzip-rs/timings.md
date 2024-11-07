# Timings #

|  method   |  impl   | release  | rel+lto | unzip  |  7zip  |
|-----------|---------|----------|---------|--------|--------|
| shrink    | native  |    0.221 |   0.176 | FAIL   | ////// |
| reduce    | native  |    9.884 |   9.886 | ////// | ////// |
| implode   | native  |    6.799 |   8.320 |  2.775 |  3.184 |
| deflate   | zlib[1] |    2.351 |   2.351 | 20.547 | 14.216 |
| deflate64 | native  |   11.315 |   7.735 | 21.111 |  8.410 |
| bzip2     | libbz2  |   21.509 |  21.509 | 42.929 | 12.954 |
| lzma      | liblzma |   21.098 |  21.098 | ////// | 19.345 |
| xz        | liblzma |   16.731 |  16.731 | ////// | 15.560 |
| zstd      | libzstd |    1.390 |   1.390 | ////// |  2.212 |
| zipcrypto | native  |   34.721 |  34.639 | 64.232 | 36.039 |
| aes128[2] | native  |    3.775 |   3.581 | ////// | 10.788 |
| aes192[2] | native  |    4.795 |   4.788 | ////// | 12.413 |
| aes256[2] | native  |    5.685 |   5.482 | ////// | 14.147 |

Note: a much different archive was used for each compression method, therefore
comparisons should on only be done per method, i.e. horizontally

[1]: The timings in native mode are 12.994 (release) and 8.892 (rel+lto)

[2]: Compression method is 0 (stored)
