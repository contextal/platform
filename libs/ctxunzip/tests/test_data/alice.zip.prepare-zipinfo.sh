#!/usr/bin/env bash
#
# The script parses `zipinfo` output for one particular example-zip-file into a json.
# It doesn't mean to support/parse all the possible outputs of `zipinfo`, but it
# should be not too complex to extend the script to support wider sets of inputs.
#
# The script was used to prepare `alice.zip.zipinfo` in a following way:
# ```
# alice.zip.prepare-zipinfo.sh ./alice.zip | jq . > ./alice.zip.zipinfo
# ```
#
# `jq` in the pipeline above is necessary for json validation and formatting


set -eo pipefail

file=${1:?path to a zip file is a mandatory argument}

command -v gsed &> /dev/null && sed="gsed" || sed="sed"

{ echo "[{"; zipinfo -v "$file"; echo "}]"; } |
    $sed -E '/^$/d' |
    $sed -E '/Central directory entry #/{N;N;/There are an extra [^ ]* bytes preceding this file/!s!(.*)\n!\1\n  There are an extra 0 bytes preceding this file.\n!}' |
    $sed -E '/There are an extra [^ ]* bytes preceding this file/{N;s!(\n  )([^\n]*)$!\1filename: \2!}' |
    $sed -E '/Central directory entry #/{N;s!.*!},\n{!}' |
    $sed -E 's!There are an extra ([^ ]*) bytes preceding this file\.!entry_type: entry\n  extra_preceding: \1!
             /offset of local header from start of archive/{N;s![^ ].*: *!local_header_offset: !;s!\n.*!!}
             s!file system or operating system of origin: *!fs_or_os_of_origin: !
             s!version of encoding software: *!ver_made_by: !
             s!minimum file system compatibility required: *!minimum_fs_required: !
             s!minimum software version required to extract: *!ver_to_extract: !
             s!compression method: *!compression_method: !
             s!file security status: *!encryption: !
             s!extended local header: *!extended_local_header: !
             s!file last modified on \(DOS date/time\): *!mtime: !
             s!32-bit CRC value \(hex\): *!crc32: 0x!
             s!uncompressed size: *([^ ]*) .*!uncompressed_size: \1!
             s!compressed size: *([^ ]*) .*!compressed_size: \1!
             s!MS-DOS file attributes \(([^ ]*) hex\): .*!msdos_external_attributes: 0x\1!
             s!Unix file attributes \(([^ ]*) octal\): .*!unix_file_attributes: 0o\1!
             s!length of filename: *([^ ]*) .*!file_name_length: \1!
             s!length of extra field: *([^ ]*) .*!extras_len: \1!
             s!length of file comment: *([^ ]*) .*!file_comment_length: \1!
             s!disk number on which file begins: *disk *!disk_number_from_one: !
             s!apparent file type: *!file_type: !
             s!non-MSDOS external file attributes: *([^ ]*) hex!non_msdos_external_attributes: 0x\1!
             /There is no zipfile comment\./d
             /There is no file comment\./d
             s!size of sliding dictionary \(implosion\): *!implosion_sliding_dictionary: !
             s!number of Shannon-Fano trees \(implosion\): *!implosion_sf_trees: !
             s!compression sub-type \(deflation\): *!deflation_sub_type: !
             /^  file last modified on \(UT extra field modtime\)/d
             /^    The local extra field has UTC\/GMT modification\/access times\./d
             /^  The central-directory extra field contains:/d
             s!^Archive: *!  entry_type: archive\n  filename: !
             /^End-of-central-directory record:/{N;d}
             s!Zip archive file size: *([^ ]*) .*!zip_archive_size: \1!
             s!Actual end-cent-dir record offset: *([^ ]*) .*!actual_end_cent_dir_offset: \1!
             s!Expected end-cent-dir record offset: *([^ ]*) .*!expected_end_cent_dir_offset: \1!
             /\(based on the length of the central directory and its expected offset\)/d
             /This zipfile constitutes/{N;s!\n *! !;s![^ ].*contains ([^ ]*) entries\.!entries: \1!}
             /The central directory is/{N;N;s![^ ].*central directory is ([^ ]*) [^ ]* bytes.*its \(expected\) offset.* is ([^ ]*) .*!cent_dir_size: \1\n  cent_dir_offset: \2!} ' |
    $sed -E '/There is a local extra field with ID.*[^.]$/{N;s!\n *! !}
             s!There is a local extra field with ID ([^ ]*) .* and ([^ ]*) data bytes.*!local_extra_field_id: \1\n  local_extra_field_\1_data_len: \2!' |
    $sed -E ':a /^  -.*[^.]$/{N;s!\n *! !;ta}' |
    $sed -E '/^  -.*\.$/s![^ ].*A subfield with ID ([^ ]*) .* and ([^ ]*) data bytes\.  The first 20 are: *(.*)\.$!subfield_id: \1\n  subfield_\1_data_len: \2\n  subfield_\1_first_20_bytes: \3!' |
    $sed -E '/^  -.*\.$/s![^ ].*A subfield with ID ([^ ]*) .* and ([^ ]*) data bytes: *(.*)\.$!subfield_id: \1\n  subfield_\1_data_len: \2\n  subfield_\1_bytes: \3!' |
    $sed -E '/^  -.*\.$/s![^ ].*A subfield with ID ([^ ]*) .* and ([^ ]*) data bytes\.$!subfield_id: \1\n  subfield_\1_data_len: \2!' |
    $sed -E ':a $!{N; ba}; s![{]([^}]*)(  subfield_id: )([^\n]*)([^}]*)\2([^\n]*)\n([^}]*)[}]!{\1\2\3 \5\4\6}!g' |
    $sed -E 's!^( *)([^:]*): (.*)!\1"\2": "\3",!' |
    $sed -E ':a $!{N; ba}; s!,(\n})!\1!g'
