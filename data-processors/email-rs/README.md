# Email parsing library and backend #

The backend parses mail objects in RFC 5322 format

The focus of the code is towards data and metadata extraction in a way that is more
conformant to the existing MUAs behaviour than to the specifications


## Supported features ##
* Header decoding: minimal parsing, generic validation
* RFC 2047 and RFC 2184 header character set decoding
* Body decoding (identity, quoted-printable, base64)
* MIME multipart support (each concrete part become a child object)
* Charset aware text conversion to UTF-8 (with replacement) of all inline part bodies
* Massive extraction of metadata, anomalies and flaws

## Unsupported features ##
* Header specific syntax check and validation (except for *Content-Type* and *Date*)
* RFC 2184 header continuation - that's in line with most MUAs
* UTF-7 text conversion
