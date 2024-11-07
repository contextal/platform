# odf-rs

## Notes

The "mimetype" file shall be the first file of the zip file. It shall not be compressed, and it shall not
use an 'extra field' in its header

The purpose is to allow the type of document represented by the package to be discovered
through 'magic number' mechanisms, such as Unix's file/magic utility. If a Zip file contains a file at
the beginning of the file that is uncompressed, and has no extra data in the header, then its file
name and data can be found at fixed positions from the beginning of the package. More
specifically, one will find:
• the string 'PK' at position 0 of all zip files
• the string 'mimetype' beginning at position 30
• the media type itself beginning at position 38