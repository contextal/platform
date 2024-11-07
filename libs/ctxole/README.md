# A library to read Ole files

Provide functionality to read objects in the *Compound File Binary Format*

The implementation, which is based entirely upon
[\[MS-CFB\]](https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/53989ce4-7b05-4f8d-829b-d08d6148375b), is
mostly focused towards malware analysis. For this reason it tries its best to mimic
the empirically determinated behaviour of MS products: this includes accepting
malformed (when not intentionally evil) content
